// Real JupyterLab/Notebook checks against an isolated candidate installation.
// 在隔离安装中检查实际 JupyterLab、Notebook 与关闭页面后的 WASM 释放。
const fs=require('fs'),path=require('path'),assert=require('node:assert/strict');
const {chromium}=require('../qa/node_modules/playwright');
const config=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const out=path.join(config.scratch,'browser');fs.mkdirSync(out,{recursive:true});
const report={platform:process.platform,steps:[],memory:[]};
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const context=await browser.newContext({viewport:{width:1400,height:950}});
 const page=await context.newPage();page.setDefaultTimeout(90000);const errors=[];page.on('pageerror',e=>errors.push({message:e.message,stack:e.stack,after:report.steps.at(-1)??'startup'}));
 async function frame(){
  await page.waitForFunction(()=>window.jupyterapp,undefined,{timeout:90000});
  await page.evaluate(()=>window.jupyterapp.restored);
  const locator=page.locator('iframe[data-testid="fitsviewer-frame"]:visible').last();await locator.waitFor({timeout:60000});
  const handle=await locator.elementHandle();const f=await handle.contentFrame();await handle.dispose();
  assert.ok(f,'Viewer iframe has no content frame');
  await f.waitForFunction(()=>window.__fitsviewerState?.loaded && !window.__fitsviewerState?.loading,undefined,{timeout:60000});
  return f;
 }
 async function open(name){await page.evaluate(async name=>{await window.jupyterapp.commands.execute('docmanager:open',{path:name,factory:'FitsViewer'});},name);return frame();}
 async function state(f){return f.evaluate(()=>window.__fitsviewerState);}
 async function close(){
  await page.evaluate(async()=>{for(const w of Array.from(window.jupyterapp.shell.widgets('main'))){if(w.context){await w.context.ready;if(!w.context.model.readOnly)await w.context.save();}w.close();await new Promise(resolve=>requestAnimationFrame(()=>requestAnimationFrame(resolve)));}});
  await page.waitForFunction(()=>document.querySelectorAll('iframe[data-testid="fitsviewer-frame"]').length===0);
 }
 try{
  await page.goto(config.url+'lab?token='+config.token,{waitUntil:'domcontentloaded'});
  await page.waitForFunction(()=>window.jupyterapp?.docRegistry.getWidgetFactory('FitsViewer'),undefined,{timeout:90000});
  await page.evaluate(()=>window.jupyterapp.restored);
  assert.equal(await page.evaluate(()=>window.jupyterapp.docRegistry.defaultWidgetFactory('image.FZ').name),'FitsViewer');
  assert.notEqual(await page.evaluate(()=>window.jupyterapp.docRegistry.defaultWidgetFactory('spectrum.txt').name),'FitsViewer');
  await page.getByText('中文数据',{exact:true}).first().dblclick();
  await page.getByText('观测 星场（测试）.fits.fz',{exact:true}).first().dblclick();
  let f=await frame();assert.equal((await state(f)).width,24);
  assert.match((await state(f)).path,/中文数据/);
  report.steps.push('Chinese-named .fits.fz file-browser double-click');
  await page.screenshot({path:path.join(out,'jupyter-fz-chinese.png')});
  for(const name of ['中文图像（测试）.fits','中文图像（测试）.fits.gz']){
   await page.getByText(name,{exact:true}).first().dblclick();f=await frame();assert.equal((await state(f)).width,24);assert.match((await state(f)).path,/中文图像/);
  }
  report.steps.push('Chinese ordinary and gzip FITS file-browser double-click');
  await page.getByText('观测 星场（测试）.fits',{exact:true}).first().dblclick();
  f=await frame();assert.ok((await state(f)).spectrum);
  report.steps.push('Chinese-named FITS spectrum file-browser double-click');
  await context.grantPermissions(['clipboard-read','clipboard-write'],{origin:new URL(config.url).origin});
  let inputBox=await f.locator('canvas').boundingBox();
  await page.mouse.click(inputBox.x+350,inputBox.y+12);
  await f.evaluate(text=>navigator.clipboard.writeText(text),'"中文数据/中文图像（测试）.fits.gz"');
  await page.keyboard.press('Control+A');await page.keyboard.press('Control+V');await page.keyboard.press('Enter');
  await f.waitForFunction(()=>window.__fitsviewerState?.path.includes('中文图像') && window.__fitsviewerState.loaded && window.__fitsviewerState.height===16 && !window.__fitsviewerState.loading && !window.__fitsviewerState.error);
  assert.equal((await state(f)).height,16);report.steps.push('Canvas path field: quoted Chinese gzip path through Ctrl+V');
  f=await open('science.fits');
  let box=await f.locator('canvas').boundingBox();
  await page.mouse.click(box.x+box.width*.5,box.y+box.height*.6);
  await page.keyboard.press('2');await f.waitForFunction(()=>window.__fitsviewerState.stretch==='Log');
  await page.mouse.move(box.x+box.width*.5,box.y+box.height*.6);
  await page.mouse.down({button:'right'});await page.mouse.move(box.x+box.width*.5+65,box.y+box.height*.6-50,{steps:8});await page.mouse.up({button:'right'});
  await f.waitForFunction(()=>window.__fitsviewerState.bias<.5 && window.__fitsviewerState.contrast>1);
  await page.keyboard.press('R');await f.waitForFunction(()=>window.__fitsviewerState.bias===.5 && window.__fitsviewerState.contrast===1);
  report.steps.push('Image stretch and right-drag brightness/contrast regression');
  const layout=[];
  for(const width of [1400,850,1100]){
   await page.setViewportSize({width,height:950});await page.waitForTimeout(200);
   const before=await state(f);box=await f.locator('canvas').boundingBox();
   const [left,top,right,bottom]=before.view_rect;
   const originX=(left+right)/2+before.pan[0]-before.width*before.zoom/2;
   const originY=(top+bottom)/2+before.pan[1]-before.height*before.zoom/2;
   for(const [x,y,invalid] of [[100,100,false],[20,20,true],[200,150,false]]){
    await page.mouse.move(box.x+originX+(x+.5)*before.zoom,box.y+originY+(before.height-y-.5)*before.zoom);
    await page.waitForTimeout(100);const after=await state(f);
    assert.equal(after.status_height,40);assert.deepEqual(after.view_rect,before.view_rect);assert.equal(after.zoom,before.zoom);
    if(invalid)assert.match(after.cursor,/invalid/);
   }
   layout.push({width,status_height:before.status_height,stable:true});
  }
  report.layout=layout;await page.setViewportSize({width:1400,height:950});
  // Let Jupyter's responsive toolbar finish its asynchronous resize before disposal.
  // 等待 Jupyter 工具栏的异步缩放完成，再关闭文档。
  await page.waitForTimeout(500);
  await close();
  // Also check opening a spectrum through JupyterLab's file URL.
  await page.goto(config.url+'lab/tree/spectrum_1d.fits',{waitUntil:'domcontentloaded'});
  f=await frame();assert.ok((await state(f)).spectrum);assert.equal((await state(f)).width,8);
  let initial=(await state(f)).plot_bounds;box=await f.locator('canvas').boundingBox();
  await page.mouse.move(box.x+box.width*.6,box.y+box.height*.6);await page.mouse.wheel(0,-150);
  await f.waitForFunction(initial=>window.__fitsviewerState.plot_bounds[1]-window.__fitsviewerState.plot_bounds[0]<initial[1]-initial[0],initial);
  const beforePan=(await state(f)).plot_bounds;
  await page.mouse.down();await page.mouse.move(box.x+box.width*.6+40,box.y+box.height*.6,{steps:5});await page.mouse.up();
  await f.waitForFunction(before=>window.__fitsviewerState.plot_bounds[0]!==before[0],beforePan);
  report.steps.push('1D FITS spectrum: units, plot, wheel zoom and pan');
  await page.screenshot({path:path.join(out,'jupyter-spectrum.png')});
  f=await open('spectrum_vector.fits');assert.equal((await state(f)).spectrum.error_column,2);
  f=await open('spectrum_loglam.fits');assert.equal((await state(f)).spectrum.uncertainty_kind,'inverse variance');
  f=await open('中文数据/观测 星场（测试）.txt');assert.ok((await state(f)).spectrum);
  report.steps.push('Scalar/vector table spectra, log10 coordinates and text Open with FitsViewer');
  await close();
  await page.goto(config.url+'lab/tree/demo.ipynb',{waitUntil:'domcontentloaded'});
  const editor=page.locator('.jp-CodeCell .cm-content').first();await editor.waitFor({timeout:60000});
  for(let i=0;i<120;i++){
   const sessions=await (await page.request.get(config.url+'api/sessions')).json();
   if(sessions.some(s=>s.path==='demo.ipynb' && s.kernel.execution_state==='idle'))break;
   if(i===119)throw Error('Candidate Notebook kernel not idle');await page.waitForTimeout(500);
  }
  await editor.click();await page.keyboard.press('Shift+Enter');
  const embedded=page.locator('.jp-OutputArea iframe');await embedded.waitFor({timeout:60000});
  const embeddedHandle=await embedded.elementHandle();const inline=await embeddedHandle.contentFrame();await embeddedHandle.dispose();
  assert.ok(inline,'Notebook iframe has no content frame');await inline.waitForFunction(()=>window.__fitsviewerState?.loaded,undefined,{timeout:60000});
  assert.ok((await state(inline)).spectrum);
  await page.screenshot({path:path.join(out,'notebook-spectrum.png')});report.steps.push('Executed fitsviewer.show in the candidate Notebook kernel under a URL prefix');
  await close();
  const cdp=await context.newCDPSession(page);
  // Create the holder in the parent realm; a child-created array pins its iframe.
  // 容器在父页面创建，避免测试对象的原型反向保留 iframe。
  await page.evaluate(()=>{window.__candidateRefs=[];});
  for(let cycle=0;cycle<4;cycle++){
   for(const name of ['rice_1_row.fits.fz','spectrum_vector.fits','中文数据/观测 星场（测试）.txt']){
    f=await open(name);
    await f.evaluate(async()=>{const m=await import(new URL('./pkg/fitsviewer_web.js',location.href));const w=await m.default();window.top.__candidateRefs.push(new window.top.WeakRef(w.memory));});
   }
   await close();await cdp.send('HeapProfiler.collectGarbage');await page.waitForTimeout(350);await cdp.send('HeapProfiler.collectGarbage');
   const alive=await page.evaluate(()=>window.__candidateRefs.filter(r=>r.deref()).length);
   const heap=await cdp.send('Runtime.getHeapUsage');
   report.memory.push({cycle:cycle+1,live_wasm_instances:alive,js_heap_bytes:heap.usedSize,backing_storage_bytes:heap.backingStorageSize??null});
   assert.equal(alive,0,'Closed views retained WebAssembly memory');
  }
  report.javascript_errors=errors;assert.deepEqual(errors,[]);report.passed=true;
 }catch(error){report.passed=false;report.error=error.stack;await page.screenshot({path:path.join(out,'failure.png')}).catch(()=>{});throw error;}
 finally{fs.writeFileSync(path.join(out,'validation.json'),JSON.stringify(report,null,2));await browser.close();}
 console.log(JSON.stringify(report));
})().catch(error=>{console.error(error.stack);process.exitCode=1;});
