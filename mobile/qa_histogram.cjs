// Focused mobile regression for hidden canvas geometry and saved Log counts.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('playwright');
const url=process.argv[2],out=path.resolve(process.argv[3]);fs.mkdirSync(out,{recursive:true});
const card=(k,v)=>(k.padEnd(8)+(v===undefined?'':'= '+v)).padEnd(80);
const header=[card('SIMPLE','                    T'),card('BITPIX','                   16'),card('NAXIS','                    2'),card('NAXIS1','                   64'),card('NAXIS2','                   64'),card('END')].join('').padEnd(2880);
const pixels=Buffer.alloc(8640);for(let i=0;i<4096;i++)pixels.writeInt16BE(i,i*2);fs.writeFileSync(out+'/fixture.fits',Buffer.concat([Buffer.from(header),pixels]));
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const report={scope:'Only mobile Histogram geometry and Log counts persistence; Chromium touch emulation',checks:[]};
 try{for(const [width,height,dpr] of [[390,844,2],[844,390,2]]){
  const context=await browser.newContext({viewport:{width,height},deviceScaleFactor:dpr,isMobile:true,hasTouch:true});const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(String(e)));
  const state=()=>page.evaluate(()=>window.FitsViewer.state());
  async function load(){await page.waitForFunction(()=>window.FitsViewer?.state(),null,{timeout:120000});await page.locator('#file').setInputFiles(out+'/fixture.fits');await page.waitForFunction(()=>window.FitsViewer.state().loaded&&window.FitsViewer.state().texture);await page.evaluate(()=>window.FitsViewer.command('histogram',{open:true}));await page.waitForTimeout(350);}
  async function tapLog(value){const rect=(await state()).histogram_controls.log_counts_rect,canvas=await page.locator('canvas').boundingBox();await page.touchscreen.tap(canvas.x+(rect[0]+rect[2])/2,canvas.y+(rect[1]+rect[3])/2);await page.waitForFunction(expected=>window.FitsViewer.state().preferences.histogram_log_counts===expected,value,{timeout:3000});}
  await page.goto(url);await load();let initial=await state();assert.equal(initial.preferences.histogram_log_counts,false);assert.equal(initial.build_features.pro,false);
  const before=initial.preferences.window_layouts.image.Histogram;assert.ok(before.size[0]>=300&&before.size[1]>=96);assert.equal(before.collapsed,false);
  await page.screenshot({path:out+`/default-${width}x${height}-dpr${dpr}.png`});
  await tapLog(true);await page.evaluate(()=>document.querySelector('#stage').style.display='none');await page.waitForTimeout(500);
  await page.evaluate(()=>document.querySelector('#stage').style.display='');await page.waitForTimeout(500);
  const after=(await state()).preferences.window_layouts.image.Histogram;
  fs.writeFileSync(out+`/hidden-${width}x${height}.json`,JSON.stringify({before,after},null,2));
  await page.screenshot({path:out+`/restored-${width}x${height}-dpr${dpr}.png`});
  for(let i=0;i<4;i++)assert.ok(Math.abs(before.rect[i]-after.rect[i])<2,'Hidden canvas preserves Histogram geometry: '+JSON.stringify({before,after}));
  await page.screenshot({path:out+`/restored-${width}x${height}-dpr${dpr}.png`});
  await page.reload();await load();assert.equal((await state()).preferences.histogram_log_counts,true);
  await tapLog(false);await page.reload();await load();assert.equal((await state()).preferences.histogram_log_counts,false);
  const prefs=(await state()).preferences;prefs.window_layouts.image.Histogram={open:true,collapsed:false,rect:[8,8,202,110],size:[180,64]};
  await page.evaluate(value=>window.FitsViewer.command('preferences',value),prefs);await page.waitForTimeout(500);
  const legacy=(await state()).preferences.window_layouts.image.Histogram;assert.ok(legacy.size[0]>=300&&legacy.size[1]>=96,'Old tiny saved sizes migrate');
  await page.screenshot({path:out+`/legacy-repaired-${width}x${height}-dpr${dpr}.png`});assert.deepEqual(errors,[]);
  report.checks.push({width,height,dpr,before,after,defaultComplete:true,proExcluded:true,logEnabledSurvivesReload:true,logDisabledSurvivesReload:true,legacyRepaired:legacy,errors});await context.close();
 }}finally{await browser.close();fs.writeFileSync(out+'/result.json',JSON.stringify(report,null,2));}
})().catch(e=>{console.error(e);process.exitCode=1;});
