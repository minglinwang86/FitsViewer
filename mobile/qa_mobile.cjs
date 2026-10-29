// Mobile-specific browser checks. No scientific oracle or desktop acceptance claims.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),zlib=require('node:zlib');
const {chromium}=require('playwright');
const url=process.argv[2],out=path.resolve(process.argv[3]);fs.mkdirSync(out,{recursive:true});
const card=(key,value)=>(key.padEnd(8)+(value===undefined?'':'= '+value)).padEnd(80);
let header=[card('SIMPLE','                    T'),card('BITPIX','                   16'),card('NAXIS','                    2'),card('NAXIS1','                   64'),card('NAXIS2','                   64'),card('END')].join('').padEnd(2880);
const pixels=Buffer.alloc(8640);for(let i=0;i<4096;i++)pixels.writeInt16BE(i,i*2);
const fixture=Buffer.concat([Buffer.from(header),pixels]);fs.writeFileSync(out+'/fixture.fits',fixture);fs.writeFileSync(out+'/fixture.fits.gz',zlib.gzipSync(fixture));
const extension=[card('XTENSION',"'IMAGE   '"),card('BITPIX','                   16'),card('NAXIS','                    2'),card('NAXIS1','                    8'),card('NAXIS2','                    8'),card('PCOUNT','                    0'),card('GCOUNT','                    1'),card('EXTNAME',"'SECOND'"),card('END')].join('').padEnd(2880);
fs.writeFileSync(out+'/fixture-mef.fits',Buffer.concat([fixture,Buffer.from(extension),Buffer.alloc(2880)]));
fs.writeFileSync(out+'/spectrum.dat',Array.from({length:40},(_,i)=>`${4000+i*10} ${10+Math.sin(i/5)}`).join('\n'));
const fd=fs.openSync(out+'/oversize.fits','w');fs.ftruncateSync(fd,64*1024*1024+1);fs.closeSync(fd);
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const report={kind:'Chromium mobile touch emulation; not physical Android/iPhone acceptance',checks:[]};
 try{for(const [width,height,dpr] of [[390,844,1],[390,844,2],[844,390,2],[393,852,3]]){
  const context=await browser.newContext({viewport:{width,height},deviceScaleFactor:dpr,isMobile:true,hasTouch:true});
  const page=await context.newPage(),errors=[],requests=[];page.on('pageerror',e=>errors.push(String(e)));page.on('request',r=>requests.push(r.url()));
  await page.goto(url);await page.waitForFunction(()=>window.FitsViewer?.state(),null,{timeout:120000});
  await page.waitForFunction(()=>Math.abs(document.querySelector('canvas').width-document.querySelector('canvas').clientWidth*devicePixelRatio)<=1);
  await page.locator('#file').setInputFiles(out+'/fixture.fits');
  await page.waitForFunction(()=>window.FitsViewer.state().loaded&&window.FitsViewer.state().texture,null,{timeout:60000});
  const state=()=>page.evaluate(()=>window.FitsViewer.state());let loaded=await state();
  assert.equal(loaded.mode,'Minimal');assert.equal(loaded.build_features.pro,false);assert.equal(loaded.width,64);assert.equal(loaded.height,64);
  assert.ok(loaded.view_rect[3]-loaded.view_rect[1]>50,'Usable image viewport');assert.ok(Math.abs(loaded.view_rect[2]-width)<2,'DPR preserves CSS coordinates');
  await page.evaluate(()=>window.FitsViewer.command('histogram',{open:true}));await page.waitForTimeout(300);
  const windowBefore=(await state()).preferences.window_layouts.image.Histogram;
  await page.evaluate(()=>document.querySelector('#stage').style.display='none');await page.waitForTimeout(300);
  await page.evaluate(()=>document.querySelector('#stage').style.display='');await page.waitForTimeout(300);
  const windowAfter=(await state()).preferences.window_layouts.image.Histogram;
  for(let i=0;i<4;i++)assert.ok(Math.abs(windowBefore.rect[i]-windowAfter.rect[i])<2,'Hidden canvas must preserve Histogram position and size');
  await page.evaluate(()=>window.FitsViewer.command('histogram',{open:false}));
  const rejected=await page.evaluate(async()=>{try{await window.FitsViewer.command('workspace.add',{paths:['test.fits']});return false;}catch{return true;}});assert.ok(rejected);
  const box=await page.locator('canvas').boundingBox(),r=loaded.view_rect,x=(r[0]+r[2])/2+box.x,y=(r[1]+r[3])/2+box.y;
  const cdp=await context.newCDPSession(page);await cdp.send('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x,y,id:1}]});
  for(let i=1;i<=5;i++){await cdp.send('Input.dispatchTouchEvent',{type:'touchMove',touchPoints:[{x:x+6*i,y:y+4*i,id:1}]});await page.waitForTimeout(25);}
  await cdp.send('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});await page.waitForTimeout(150);
  const afterPan=await state();assert.ok(Math.abs(afterPan.pan[0])+Math.abs(afterPan.pan[1])>10,'Single-finger drag must pan');
  await page.evaluate(()=>window.FitsViewer.command('aperture.add',{name:'mobile-check',shape:{kind:'circle',center:[12.5,23.5],radius:3},background:null,exclude:[]}));
  await page.waitForFunction(()=>window.FitsViewer.state().apertures.length===1);
  const apertureBefore=(await state()).apertures;assert.equal(apertureBefore.length,1);
  async function tap(id){const b=await page.locator(id).boundingBox();await page.touchscreen.tap(b.x+b.width/2,b.y+b.height/2);await page.waitForTimeout(150);}
  await tap('#zoom-in');const zoomIn=await state();assert.ok(zoomIn.zoom>afterPan.zoom);
  await tap('#zoom-out');const zoomOut=await state();assert.ok(zoomOut.zoom<zoomIn.zoom);assert.deepEqual(zoomOut.apertures,apertureBefore,'Display gestures preserve measurement coordinates');
  await tap('#fit');assert.deepEqual((await state()).pan,[0,0]);
  await page.locator('#file').setInputFiles(out+'/fixture-mef.fits');await page.waitForFunction(()=>window.FitsViewer.state().path.endsWith('/fixture-mef.fits')&&window.FitsViewer.state().hdu===0&&window.FitsViewer.state().texture);
  await page.evaluate(()=>window.FitsViewer.command('open',{path:window.FitsViewer.state().path,hdu:1,destination:'replace'}));await page.waitForFunction(()=>window.FitsViewer.state().hdu===1&&window.FitsViewer.state().width===8&&window.FitsViewer.state().texture);
  await page.evaluate(()=>window.FitsViewer.command('header.select',{hdu:1}));await page.waitForTimeout(100);
  await page.screenshot({path:out+`/header-${width}x${height}-dpr${dpr}.png`});await page.evaluate(()=>window.FitsViewer.command('action',{name:'Header'}));
  await page.locator('#file').setInputFiles(out+'/spectrum.dat');await page.waitForFunction(()=>window.FitsViewer.state().path.endsWith('/spectrum.dat')&&window.FitsViewer.state().spectrum);
  const plotBefore=(await state()).plot_bounds;await tap('#zoom-in');const plotAfter=(await state()).plot_bounds;assert.ok(plotAfter[1]-plotAfter[0]<plotBefore[1]-plotBefore[0],'Spectrum zoom button');
  await tap('#fit');await page.screenshot({path:out+`/spectrum-${width}x${height}-dpr${dpr}.png`});
  await page.locator('#file').setInputFiles(out+'/fixture.fits.gz');await page.waitForFunction(()=>window.FitsViewer.state().path.endsWith('/fixture.fits.gz')&&window.FitsViewer.state().texture);
  await page.locator('#file').setInputFiles(out+'/oversize.fits');await page.waitForFunction(()=>document.querySelector('#status').textContent.includes('64 MiB'));
  assert.ok((await state()).path.endsWith('/fixture.fits.gz'),'Oversize rejection preserves active document');
  await page.screenshot({path:out+`/mobile-${width}x${height}-dpr${dpr}.png`});
  await page.evaluate(()=>navigator.serviceWorker.ready);await page.reload();await page.waitForFunction(()=>window.FitsViewer?.state(),null,{timeout:120000});
  await context.setOffline(true);await page.reload();await page.waitForFunction(()=>window.FitsViewer?.state(),null,{timeout:120000});
  await page.locator('#file').setInputFiles(out+'/fixture.fits');await page.waitForFunction(()=>window.FitsViewer.state().loaded&&window.FitsViewer.state().texture);
  await tap('#close');assert.equal((await state()).loaded,false);
  assert.deepEqual(errors,[]);assert.ok(requests.every(u=>u.startsWith(new URL(url).origin)||u.startsWith('data:')||u.startsWith('blob:')),'No external data requests');
  report.checks.push({width,height,dpr,localFile:true,gzip:true,multiHdu:true,currentHeader:true,basicSpectrum:true,spectrumZoom:true,proExcluded:true,singleFingerPan:afterPan.pan,zoomButtons:true,fit:true,apertureCount:apertureBefore.length,hiddenRestore:true,oversizeRejected:true,offlineReload:true,close:true,errors});
  await context.close();
 }}finally{await browser.close();fs.writeFileSync(out+'/result.json',JSON.stringify(report,null,2));}
})().catch(e=>{console.error(e);process.exitCode=1;});
