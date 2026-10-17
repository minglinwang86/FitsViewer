// CDP-only page: avoid Playwright's page/network response instrumentation.
// 页面只启用必要 CDP 域，复核测试工具的网络响应缓存是否影响内存。
const fs = require('fs');
const path = require('path');
const {chromium} = require('../qa/node_modules/playwright');
const config = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const info = JSON.parse(fs.readFileSync(config.server_info, 'utf8'));
const root = path.resolve(__dirname, '..');
const pause = ms => new Promise(resolve=>setTimeout(resolve,ms));
function pss(pid){try{return Number(fs.readFileSync(`/proc/${pid}/smaps_rollup`,'utf8').match(/^Pss:\s+(\d+)/m)[1])/1024;}catch{return 0;}}
(async()=>{
  const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
  const host=await browser.newBrowserCDPSession();
  const browserContextId=(await host.send('Target.createBrowserContext')).browserContextId;
  const targetId=(await host.send('Target.createTarget',{url:'about:blank',browserContextId})).targetId;
  const sessionId=(await host.send('Target.attachToTarget',{targetId,flatten:false})).sessionId;
  let sequence=0;
  const pending=new Map(), contexts=new Map(), samples=[], errors=[];
  host.on('Target.receivedMessageFromTarget', event=>{
    if(event.sessionId!==sessionId)return;
    const message=JSON.parse(event.message);
    if(message.id){const item=pending.get(message.id);if(item){pending.delete(message.id);clearTimeout(item.timer);message.error?item.reject(Error(JSON.stringify(message.error))):item.resolve(message.result);}return;}
    if(message.method==='Runtime.executionContextCreated'){
      const ctx=message.params.context;
      if(ctx.auxData?.isDefault)contexts.set(ctx.auxData.frameId,ctx.id);
    }
    if(message.method==='Runtime.exceptionThrown')errors.push(message.params.exceptionDetails.text);
  });
  function send(method,params={}){
    const id=++sequence;
    return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{pending.delete(id);reject(Error('CDP timeout: '+method));},60000);
      pending.set(id,{resolve,reject,timer});
      host.send('Target.sendMessageToTarget',{sessionId,message:JSON.stringify({id,method,params})}).catch(reject);
    });
  }
  async function evaluate(expression,contextId){
    const result=await send('Runtime.evaluate',{expression,contextId,returnByValue:true,awaitPromise:true});
    if(result.exceptionDetails)throw Error(JSON.stringify(result.exceptionDetails));
    return result.result.value;
  }
  async function wait(expression){for(let n=0;n<300;n++){if(await evaluate(expression))return;await pause(100);}throw Error('Timed out: '+expression);}
  async function snapshot(stage){
    await send('HeapProfiler.collectGarbage');await pause(750);
    const processes=(await host.send('SystemInfo.getProcessInfo')).processInfo;
    const heap=await send('Runtime.getHeapUsage');
    let rust=0;if(fs.existsSync(config.rust_state))rust=pss(JSON.parse(fs.readFileSync(config.rust_state)).pid);
    const sample={stage,browser_pss_MiB:+processes.reduce((s,p)=>s+pss(p.id),0).toFixed(2),
      js_heap_MiB:+(heap.usedSize/2**20).toFixed(2),backing_storage_MiB:heap.backingStorageSize===undefined?null:+(heap.backingStorageSize/2**20).toFixed(2),
      rust_pss_MiB:+rust.toFixed(2),jupyter_pss_MiB:+pss(info.pid).toFixed(2),
      viewer_iframes:await evaluate(`document.querySelectorAll('iframe[data-testid="fitsview-frame"]').length`)};
    samples.push(sample);console.log(JSON.stringify(sample));
  }
  async function open(name,loaded=true){
    await evaluate(`(async()=>{await window.jupyterapp.commands.execute('docmanager:open',{path:${JSON.stringify(name)},factory:'FITSView'});return true;})()`);
    if(!loaded)return;
    const selector=`iframe[data-testid="fitsview-frame"][src*="${name}"]`;
    await wait(`document.querySelector(${JSON.stringify(selector)})?.contentWindow?.__fitsviewState?.loaded === true`);
    const tree=(await send('Page.getFrameTree')).frameTree;
    const child=tree.childFrames.find(f=>f.frame.url.includes(name));
    const contextId=contexts.get(child.frame.id);
    if(!contextId)throw Error('Missing child execution context');
    await evaluate(`(async()=>{
      const m=await import(new URL('./pkg/fitsview_web.js',location.href));const w=await m.default();
      window.top.__memoryRefs.push(new window.top.WeakRef(w.memory));
      window.addEventListener('fitsview-dispose',()=>{const c=document.getElementById('fitsview-canvas');const gl=c.getContext('webgl2')||c.getContext('webgl');window.top.postMessage({type:'memory-release',disposed:window.__fitsviewDisposed===true,lost:gl?.isContextLost()??false},location.origin);},{once:true});
      return true;})()`,contextId);
  }
  async function close(){
    await evaluate(`(()=>{for(const w of Array.from(window.jupyterapp.shell.widgets('main')))w.close();return true;})()`);
    await wait(`document.querySelectorAll('iframe[data-testid="fitsview-frame"]').length===0`);
    await pause(500);
  }
  try{
    await send('Runtime.enable');await send('Page.enable');
    await send('Page.navigate',{url:info.url+'lab?token='+info.token});
    await wait(`Boolean(window.jupyterapp?.docRegistry.getWidgetFactory('FITSView'))`);
    await evaluate(`(()=>{window.__memoryRefs=[];window.__memoryReleases=[];window.addEventListener('message',e=>{if(e.origin===location.origin&&e.data?.type==='memory-release')window.__memoryReleases.push(e.data);});return true;})()`);
    await open(config.files[0]);await close();await snapshot('warm_closed');
    for(let cycle=1;cycle<=config.cycles;cycle++){
      for(const name of config.files)await open(name);
      await snapshot(`cycle_${cycle}_open`);
      await close();await snapshot(`cycle_${cycle}_closed`);
    }
    for(let n=0;n<12;n++){await open(config.files[n%config.files.length]);await close();}
    await snapshot('twelve_single_closed');
    for(let n=0;n<8;n++){await open(config.files[n%config.files.length],false);await close();}
    await pause(10000);await snapshot('early_close_then_idle');
    const state=await evaluate(`({wasm_memories:window.__memoryRefs.filter(r=>r.deref()).length,releases:window.__memoryReleases})`);
    const closed=samples.filter(s=>/cycle_\d+_closed/.test(s.stage));
    const growth=closed.at(-1).browser_pss_MiB-closed[0].browser_pss_MiB;
    const expected=1+config.cycles*config.files.length+12;
    const report={label:config.label,wheel:config.wheel,sha256:config.sha256,driver:'CDP without Network/Debugger/Performance domains',
      simultaneous_documents:config.files.length,image_dimensions:[config.size,config.size],cycles:config.cycles,file_type:config.uncompressed?'fits':'fits.gz',pixel_type:'float32',
      single_open_close_iterations:12,early_close_iterations:8,samples,
      closed_browser_growth_MiB:+growth.toFixed(2),surviving_wasm_memories:state.wasm_memories,
      release_events:state.releases.length,expected_release_events:expected,
      all_contexts_lost:state.releases.every(e=>e.disposed&&e.lost),javascript_errors:errors,
      viewer_cleanup_verified:state.wasm_memories===0&&state.releases.length===expected&&state.releases.every(e=>e.disposed&&e.lost)&&errors.length===0};
    fs.writeFileSync(path.join(root,'reports','memory_'+config.label+'.json'),JSON.stringify(report,null,2));
    console.log(JSON.stringify({complete:true,cleanup:report.viewer_cleanup_verified,growth_MiB:report.closed_browser_growth_MiB,wasm_memories:state.wasm_memories}));
    if(!report.viewer_cleanup_verified)throw Error('Viewer teardown verification failed; inspect the memory report');
  }finally{
    for(const item of pending.values())clearTimeout(item.timer);
    await host.send('Target.disposeBrowserContext',{browserContextId}).catch(()=>{});
    await browser.close();
  }
})();
