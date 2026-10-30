// Only the changed toolbar/document-strip layout; earlier science/identity checks are reused.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('playwright');const url=process.argv[2],out=path.resolve(process.argv[3]);fs.mkdirSync(out,{recursive:true});
const card=(key,value)=>(key.padEnd(8)+(value===undefined?'':'= '+value.padStart(20))).padEnd(80);
const header=[card('SIMPLE','T'),card('BITPIX','16'),card('NAXIS','2'),card('NAXIS1','32'),card('NAXIS2','32'),card('END')].join('').padEnd(2880),pixels=Buffer.alloc(2880);for(let i=0;i<1024;i++)pixels.writeInt16BE(i,i*2);
for(const name of ['first.fits','second.fits'])fs.writeFileSync(out+'/'+name,Buffer.concat([Buffer.from(header),pixels]));
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});const report={scope:'Changed mobile ribbon/task-height and document-strip layout only',checks:[]};
 const cases=process.env.MOBILE_LAYOUT_SMOKE==='1'?[[390,844,'zh-CN']]:[[390,844,'en'],[390,844,'zh-CN'],[844,390,'en'],[844,390,'zh-CN']];
 try{for(const [width,height,language] of cases){
  const context=await browser.newContext({viewport:{width,height},deviceScaleFactor:2,isMobile:true,hasTouch:true}),page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(String(e)));
  const state=()=>page.evaluate(()=>window.FitsViewer.state()),command=(op,params={})=>page.evaluate(([op,params])=>window.FitsViewer.command(op,params),[op,params]);
  await page.goto(url);await page.waitForFunction(()=>window.FitsViewer?.state(),null,{timeout:120000});
  const prefs=(await state()).preferences;await command('preferences',{...prefs,language,minimal_open_destination:'tab',ribbon_tab:'View',ribbon_collapsed:false});
  async function load(name,count){await page.locator('#file').setInputFiles(out+'/'+name);await page.waitForFunction(count=>{const s=window.FitsViewer.state();return s.loaded&&!s.loading&&s.texture&&s.first_frame_ready&&s.documents.views.length===count;},count);await page.waitForTimeout(200);}
  function capture(s){const ribbon=s.ribbon_rect,strip=s.documents.rect,image=s.view_rect;assert.ok(ribbon&&strip&&image,'Expected real shared layout rectangles');assert.ok(strip[1]>=ribbon[3]-.5,'Document strip must be below the complete toolbar');assert.ok(image[1]>=strip[3]-.5,'Image must begin below document strip');assert.ok(strip[3]-strip[1]>10&&strip[3]-strip[1]<48,'Document strip remains one compact row');assert.equal(s.build_features.pro,false);return {ribbon,strip,image,task:s.preferences.ribbon_tab};}
  await load('first.fits',1);const one=capture(await state());await load('second.fits',2);const two=capture(await state());assert.ok(Math.abs(one.strip[1]-two.strip[1])<1&&Math.abs(one.strip[3]-two.strip[3])<1,'Opening a second document must not move or grow its strip');
  const transitions=[];
  for(const task of ['Measure','View']){
   const current=await state(),r=current.ribbon_tabs[task],canvas=await page.locator('canvas').boundingBox();assert.ok(r&&r[0]>=0&&r[2]<=canvas.width+1,'Task touch target is reachable');await page.touchscreen.tap(canvas.x+(r[0]+r[2])/2,canvas.y+(r[1]+r[3])/2);await page.waitForFunction(task=>window.FitsViewer.state().preferences.ribbon_tab===task,task);
   const immediate=capture(await state());await page.waitForTimeout(200);const settled=capture(await state());for(const sample of [immediate,settled]){assert.ok(Math.abs(sample.ribbon[3]-two.ribbon[3])<1,'Task switch must preserve toolbar height');assert.ok(Math.abs(sample.strip[1]-two.strip[1])<1&&Math.abs(sample.image[1]-two.image[1])<1,'Task switch must preserve strip/image positions');}transitions.push({task,immediate,settled});await page.screenshot({path:out+`/${width}x${height}-${language}-${task}.png`});
  }
  await command('tab.close',{index:1});await page.waitForFunction(()=>window.FitsViewer.state().documents.views.length===1);await page.waitForTimeout(200);const returned=capture(await state());assert.ok(Math.abs(returned.strip[1]-one.strip[1])<1&&Math.abs(returned.strip[3]-one.strip[3])<1,'Closing to one document must retain the strip position');assert.deepEqual(errors,[]);report.checks.push({width,height,language,dpr:2,one,two,transitions,returned,errors});await context.close();
 }}finally{await browser.close();fs.writeFileSync(out+'/result.json',JSON.stringify(report,null,2));}
})().catch(error=>{console.error(error);process.exitCode=1;});
