// Changed mobile behavior only: opaque local tabs/HDU, compact histogram and touch.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('playwright');
const url=process.argv[2],out=path.resolve(process.argv[3]);fs.mkdirSync(out,{recursive:true});
const card=(key,value)=>(key.padEnd(8)+(value===undefined?'':'= '+value)).padEnd(80);
function hdu(extension,value){
 const values=extension?[['XTENSION',"'IMAGE   '"],['BITPIX','16'],['NAXIS','2'],['NAXIS1','32'],['NAXIS2','32'],['PCOUNT','0'],['GCOUNT','1']]:[['SIMPLE','T'],['BITPIX','16'],['NAXIS','2'],['NAXIS1','32'],['NAXIS2','32'],['EXTEND','T']];
 const header=values.map(([k,v])=>card(k,v.padStart(20))).concat(card('END')).join('').padEnd(2880);const pixels=Buffer.alloc(2880);for(let i=0;i<1024;i++)pixels.writeInt16BE(value,i*2);pixels.writeInt16BE(value+1,0);return Buffer.concat([Buffer.from(header),pixels]);
}
for(const [folder,a,b] of [['a',11,111],['b',22,222],['c',33,333]]){fs.mkdirSync(out+'/'+folder,{recursive:true});fs.writeFileSync(out+'/'+folder+'/IMG.fits',Buffer.concat([hdu(false,a),hdu(true,b)]));}
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});const report={scope:'Changed Mini mobile runtime only; Chromium mobile touch emulation',checks:[]};
 try{for(const [width,height,dpr] of [[390,844,2],[844,390,2]]){
  const context=await browser.newContext({viewport:{width,height},deviceScaleFactor:dpr,isMobile:true,hasTouch:true}),page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(String(e)));
  const state=()=>page.evaluate(()=>window.FitsViewer.state());const command=(op,params={})=>page.evaluate(([op,params])=>window.FitsViewer.command(op,params),[op,params]);
  await page.goto(url);await page.waitForFunction(()=>window.FitsViewer?.state(),null,{timeout:120000});
  async function destination(value){await page.locator('#destination').selectOption(value);await page.waitForFunction(value=>window.FitsViewer.state().preferences.minimal_open_destination===value,value);}
  async function loaded(){await page.waitForFunction(()=>window.FitsViewer.state().loaded&&!window.FitsViewer.state().loading&&window.FitsViewer.state().texture);await page.waitForTimeout(120);}
  async function pick(folder,old){await page.locator('#file').setInputFiles(out+'/'+folder+'/IMG.fits');await page.waitForFunction(previous=>window.FitsViewer.state().loaded&&window.FitsViewer.state().path!==previous&&window.FitsViewer.state().texture,old||'');await loaded();return (await state()).path;}
  async function value(expected){const s=await state(),b=await page.locator('canvas').boundingBox(),r=s.view_rect;await page.mouse.move(b.x+(r[0]+r[2])/2,b.y+(r[1]+r[3])/2);await page.waitForFunction(expected=>Number(window.FitsViewer.state().cursor.match(/value=([^\s·]+)/)?.[1])===expected,expected);}
  async function select(index,expectedPath,expectedValue){await command('tab.select',{index});await page.waitForFunction(path=>window.FitsViewer.state().path===path,expectedPath);await loaded();await value(expectedValue);}
  async function hduAt(key,index,expected){await command('open',{path:key,hdu:index,destination:'replace'});await page.waitForFunction(([key,index])=>window.FitsViewer.state().path===key&&window.FitsViewer.state().hdu===index,[key,index]);await loaded();await value(expected);}
  await destination('tab');const a=await pick('a');await value(11);const b=await pick('b',a);assert.notEqual(a,b);assert.ok(a.startsWith('local://')&&b.startsWith('local://'));assert.equal((await state()).documents.views.length,2);await value(22);
  await select(0,a,11);await hduAt(a,1,111);await select(1,b,22);await hduAt(b,1,222);await select(0,a,111);
  await destination('replace');const c=await pick('c',a);await value(33);assert.equal((await state()).documents.views.length,2);await select(1,b,222);await select(0,c,33);
  const bState=await state();assert.equal(bState.build_features.pro,false);assert.ok(bState.documents.offline_source_bytes<=bState.documents.offline_source_budget);
  await page.screenshot({path:out+`/tabs-${width}x${height}.png`});
  const canvas=await page.locator('canvas').boundingBox(),r=bState.view_rect,x=canvas.x+(r[0]+r[2])/2,y=canvas.y+(r[1]+r[3])/2,cdp=await context.newCDPSession(page);
  await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x,y,id:1}]});for(let i=1;i<=4;i++){await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:x+i*6,y:y+i*4,id:1}]});await page.waitForTimeout(25);}await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await page.waitForTimeout(120);const pan=(await state()).pan;assert.ok(pan[0]>10&&pan[1]>5);
  let button=await page.locator('#zoom-in').boundingBox();const zoom=(await state()).zoom;await page.touchscreen.tap(button.x+button.width/2,button.y+button.height/2);await page.waitForFunction(zoom=>window.FitsViewer.state().zoom>zoom,zoom);
  await command('histogram',{open:true});await page.waitForTimeout(300);let s=await state(),box=s.preferences.window_layouts.image.Histogram,controls=s.histogram_controls;
  assert.ok(box.size[0]>=300&&box.size[0]<=335&&box.size[1]>=96&&box.size[1]<=160);const aspect=(box.rect[2]-box.rect[0])/(box.rect[3]-box.rect[1]);assert.ok(aspect>=1.65&&aspect<=1.95);
  for(const rect of [box.rect,controls.plot_rect,controls.log_counts_rect,controls.details_rect]){assert.ok(rect[0]>=0&&rect[1]>=0&&rect[2]<=canvas.width+1&&rect[3]<=canvas.height+1,'Controls stay within the mobile canvas');}
  assert.ok(controls.log_counts_rect[1]>=box.rect[1]&&controls.log_counts_rect[3]<=box.rect[1]+32,'Log is in the title');
  await page.screenshot({path:out+`/compact-${width}x${height}.png`});
  async function tapRect(r){await page.touchscreen.tap(canvas.x+(r[0]+r[2])/2,canvas.y+(r[1]+r[3])/2);await page.waitForTimeout(150);}
  await tapRect(controls.log_counts_rect);await page.waitForFunction(()=>window.FitsViewer.state().preferences.histogram_log_counts);
  await tapRect((await state()).histogram_controls.details_rect);await page.waitForFunction(()=>window.FitsViewer.state().histogram_controls.details_open);await page.screenshot({path:out+`/details-${width}x${height}.png`});await page.keyboard.press('Escape');
  assert.deepEqual(errors,[]);report.checks.push({width,height,dpr,opaqueSameNames:[a,b,c],tabCount:2,hduReturnsOriginal:[11,111,22,222,111],replaceKeepsOtherTab:true,touchPan:pan,touchZoom:true,histogram:box,compactAspect:aspect,titleLog:true,detailsMenu:true,proExcluded:true,errors});await context.close();
 }}finally{await browser.close();fs.writeFileSync(out+'/result.json',JSON.stringify(report,null,2));}
})().catch(error=>{console.error(error);process.exitCode=1;});
