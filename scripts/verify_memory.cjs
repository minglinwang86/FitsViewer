// Real document open/close tests. / 真实 Jupyter 文件页签开关与内存采样。
const fs = require('fs');
const path = require('path');
const assert = require('node:assert/strict');
const {chromium} = require('../qa/node_modules/playwright');
const config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const server = JSON.parse(fs.readFileSync(config.server_info, 'utf8'));
const root = path.resolve(__dirname, '..');
function memory(pid) {
  try {
    const text = fs.readFileSync(`/proc/${pid}/smaps_rollup`, 'utf8');
    return {rss: Number(text.match(/^Rss:\s+(\d+)/m)[1]) / 1024, pss: Number(text.match(/^Pss:\s+(\d+)/m)[1]) / 1024};
  } catch { return {rss: 0, pss: 0}; }
}
(async () => {
  const browser = await chromium.launch({headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage', '--use-angle=swiftshader', '--enable-unsafe-swiftshader']});
  const context = await browser.newContext({viewport: {width: 1200, height: 850}});
  const page = await context.newPage();
  const errors = [], warnings = [], samples = [];
  page.on('pageerror', e => errors.push(e.message));
  page.on('console', m => {if (/too many active|context lost/i.test(m.text())) warnings.push(m.text());});
  const host = await browser.newBrowserCDPSession();
  const cdp = await context.newCDPSession(page);
  await cdp.send('Performance.enable');
  await cdp.send('HeapProfiler.enable');
  async function snapshot(stage, collect = false) {
    if (collect) {await cdp.send('HeapProfiler.collectGarbage'); await page.waitForTimeout(750);}
    const dom = await cdp.send('Memory.getDOMCounters');
    const processes = (await host.send('SystemInfo.getProcessInfo')).processInfo;
    const totals = {rss: 0, pss: 0};
    for (const process of processes) {const used = memory(process.id); totals.rss += used.rss; totals.pss += used.pss;}
    const perf = (await cdp.send('Performance.getMetrics')).metrics;
    const heap = await cdp.send('Runtime.getHeapUsage');
    let rust = {rss: 0, pss: 0};
    if (fs.existsSync(config.rust_state)) rust = memory(JSON.parse(fs.readFileSync(config.rust_state)).pid);
    const sample = {stage, forced_gc: collect, browser_pss_MiB: +totals.pss.toFixed(2), browser_rss_MiB: +totals.rss.toFixed(2),
      rust_pss_MiB: +rust.pss.toFixed(2), jupyter_pss_MiB: +memory(server.pid).pss.toFixed(2),
      documents: dom.documents, nodes: dom.nodes, listeners: dom.jsEventListeners,
      js_heap_MiB: +(perf.find(m => m.name === 'JSHeapUsedSize').value / 2**20).toFixed(2),
      backing_storage_MiB: heap.backingStorageSize === undefined ? null : +(heap.backingStorageSize / 2**20).toFixed(2),
      viewer_iframes: await page.locator('iframe[data-testid="fitsview-frame"]').count()};
    samples.push(sample); console.log(JSON.stringify(sample)); return sample;
  }
  async function open(name) {
    await page.evaluate(async ({name,control}) => {await window.jupyterapp.commands.execute('docmanager:open', {path: name, factory: control?'Editor':'FITSView'});}, {name,control:config.control});
    await page.evaluate(name=>{
      const w=Array.from(window.jupyterapp.shell.widgets('main')).find(w=>w.context?.path===name);
      window.__fitsviewWeakRefs.push({widget:new WeakRef(w),content:new WeakRef(w.content),frame:w.content.iframe?new WeakRef(w.content.iframe):null});
    },name);
    if(config.control){await page.waitForFunction(name=>window.jupyterapp.shell.currentWidget?.context?.path===name,name);return null;}
    const selector = 'iframe[data-testid="fitsview-frame"][src*="' + name + '"]';
    await page.locator(selector).waitFor({state:'visible', timeout:60000});
    const handle = await page.locator(selector).elementHandle();
    const frame = await handle.contentFrame();
    await handle.dispose();
    await frame.waitForFunction(() => window.__fitsviewState?.loaded, {timeout:60000});
    assert.equal(await frame.evaluate(() => window.__fitsviewState.width), config.size);
    await frame.evaluate(async () => {
      const module = await import(new URL('./pkg/fitsview_web.js', location.href));
      const wasm = await module.default();
      window.top.__fitsviewWasmRefs.push(new window.top.WeakRef(wasm.memory));
      window.addEventListener('fitsview-dispose', () => {
        const canvas = document.getElementById('fitsview-canvas');
        const gl = canvas.getContext('webgl2') || canvas.getContext('webgl');
        window.top.postMessage({type:'fitsview-release-check',disposed:window.__fitsviewDisposed === true,context_lost:gl?.isContextLost() ?? false},location.origin);
      }, {once:true});
    });
    return frame;
  }
  async function closeAll() {
    await page.evaluate(() => {for (const widget of Array.from(window.jupyterapp.shell.widgets('main'))) widget.close();});
    await page.waitForFunction(() => document.querySelectorAll('iframe[data-testid="fitsview-frame"]').length === 0);
    await page.waitForTimeout(750);
  }
  try {
    await page.goto(server.url + 'lab?token=' + server.token, {waitUntil:'domcontentloaded'});
    await page.waitForFunction(() => window.jupyterapp?.docRegistry.getWidgetFactory('FITSView'), {timeout:60000});
    await page.evaluate(() => {
      window.__fitsviewReleases = [];
      window.__fitsviewWeakRefs = [];
      window.__fitsviewWasmRefs = [];
      window.addEventListener('message', event => {
        if(event.origin === location.origin && event.data?.type === 'fitsview-release-check') window.__fitsviewReleases.push(event.data);
      });
    });
    await open(config.files[0]);
    const frontend = await page.evaluate(() => Array.from(window.jupyterapp.shell.widgets('main')).filter(w=>w.content?.iframe).map(w=>({methods:Object.getOwnPropertyNames(Object.getPrototypeOf(w.content)),dispose:w.content.dispose.toString().slice(0,500)})));
    if(config.probe){
      const frame = page.frames().find(f=>f.url().includes('/fitsview/'));
      const html = await frame.evaluate(()=>({cleanup_script:Array.from(document.scripts).some(s=>s.textContent.includes('fitsview-dispose')),disposed:window.__fitsviewDisposed??null}));
      const parent = await page.evaluate(()=>{
        const widget=Array.from(window.jupyterapp.shell.widgets('main')).find(w=>w.content?.iframe);
        window.__disposeProbe={widget:false,content:false,event:false};
        widget.disposed.connect(()=>window.__disposeProbe.widget=true);
        widget.content.disposed.connect(()=>window.__disposeProbe.content=true);
        try {widget.content.iframe.contentWindow.addEventListener('fitsview-dispose',()=>window.__disposeProbe.event=true);widget.content.release();return {disposed:widget.content.iframe.contentWindow.__fitsviewDisposed??null,event:window.__disposeProbe.event};}
        catch(error){return {error:String(error)};}
      });
      await closeAll();
      console.log(JSON.stringify({frontend,html,parent,close:await page.evaluate(()=>window.__disposeProbe)})); return;
    }
    await closeAll();
    await snapshot('warm_closed', true);
    for (let cycle = 1; cycle <= config.cycles; cycle++) {
      let wasm = 0;
      for (const name of config.files) {
        const frame = await open(name);
        if(frame)wasm += await frame.evaluate(async () => {const m = await import(new URL('./pkg/fitsview_web.js', location.href)); return (await m.default()).memory.buffer.byteLength;});
      }
      const peak = await snapshot('cycle_' + cycle + '_open'); peak.wasm_linear_memory_MiB = wasm / 2**20;
      await closeAll();
      await snapshot('cycle_' + cycle + '_closed_natural');
      await snapshot('cycle_' + cycle + '_closed_gc', true);
    }
    for (let n = 0; n < 12; n++) {await open(config.files[n % config.files.length]); await closeAll();}
    await snapshot('twelve_single_open_close_gc', true);
    for (let n = 0; n < 8; n++) {
      await page.evaluate(async ({name,control}) => {await window.jupyterapp.commands.execute('docmanager:open',{path:name,factory:control?'Editor':'FITSView'});},{name:config.files[n % config.files.length],control:config.control});
      await closeAll();
    }
    await snapshot('eight_early_close_gc',true);
    const popups = [];
    for (const name of (config.control?[]:config.files.slice(0, 3))) {
      const popup = await context.newPage(); popups.push(popup);
      await popup.goto(server.url + 'fitsview/?window=1&path=' + encodeURIComponent(name));
      await popup.waitForFunction(() => window.__fitsviewState?.loaded, {timeout:60000});
    }
    await snapshot('three_standalone_pages_open');
    for (const popup of popups) await popup.close();
    await snapshot('standalone_pages_closed_gc', true);
    await page.waitForTimeout(10000);
    await snapshot('idle_ten_seconds_gc',true);
    let pressure = null;
    try {
      await cdp.send('Memory.simulatePressureNotification',{level:'critical'});
      await page.waitForTimeout(2000);
      await snapshot('after_memory_pressure_gc',true);
      pressure = 'critical notification to the isolated test browser';
    } catch(error) {pressure = 'unsupported: ' + String(error);}
    const closed = samples.filter(s => /cycle_\d+_closed_gc/.test(s.stage));
    const warm = samples[0], final = samples.at(-1);
    const growth = closed.at(-1).browser_pss_MiB - closed[0].browser_pss_MiB;
    const documentGrowth = closed.at(-1).documents - warm.documents;
    const objects = await page.evaluate(()=>({widgets:window.__fitsviewWeakRefs.filter(r=>r.widget.deref()).length,contents:window.__fitsviewWeakRefs.filter(r=>r.content.deref()).length,iframes:window.__fitsviewWeakRefs.filter(r=>r.frame?.deref()).length,wasm_memories:window.__fitsviewWasmRefs.filter(r=>r.deref()).length}));
    const releases = await page.evaluate(() => window.__fitsviewReleases);
    const releaseExpected = 1 + config.files.length * config.cycles + 12;
    const report = {wheel:config.wheel, sha256:config.sha256, label:config.label, image_dimensions:[config.size,config.size],
      pixel_type:config.control?null:'float32', file_type:config.control?'text':'fits.gz', control:config.control, frontend, simultaneous_documents:config.files.length, cycles:config.cycles,
      single_open_close_iterations:12, standalone_pages:popups.length, samples,
      closed_browser_pss_growth_after_first_cycle_MiB:+growth.toFixed(2), closed_document_growth:documentGrowth,
      high_process_memory_growth: growth > 64,
      suspected_viewer_retention: !config.control && (objects.iframes > 0 || objects.wasm_memories > 0),
      javascript_errors:errors, webgl_warnings:warnings, early_close_iterations:8, pressure_probe:pressure,
      explicit_release_events:releases, explicit_releases_expected:releaseExpected,
      surviving_closed_objects:objects,
      viewer_cleanup_verified:!config.control && releases.length === releaseExpected && releases.every(e=>e.disposed&&e.context_lost) && objects.iframes===0 && objects.wasm_memories===0,
      closed_viewer_iframes:final.viewer_iframes, complete:true,
      interpretation:'PSS/RSS include allocator and browser caches; GC samples test reachability, not immediate OS reclamation.'};
    fs.writeFileSync(path.join(root,'reports','memory_' + config.label + '.json'),JSON.stringify(report,null,2));
    assert.equal(final.viewer_iframes,0); assert.deepEqual(errors,[]);
    if(!config.control)assert.ok(report.viewer_cleanup_verified,'Renderer/WASM teardown verification failed');
    console.log(JSON.stringify({complete:true,viewer_cleanup_verified:report.viewer_cleanup_verified,objects,growth_MiB:report.closed_browser_pss_growth_after_first_cycle_MiB,document_growth:documentGrowth}));
  } catch (error) {
    fs.writeFileSync(path.join(root,'reports','memory_' + config.label + '_partial.json'),JSON.stringify({samples,error:String(error)},null,2));
    throw error;
  } finally {await browser.close();}
})();
