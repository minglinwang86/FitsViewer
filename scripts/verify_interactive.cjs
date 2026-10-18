// Exercise actual canvas/HTTP integration in an isolated candidate server.
// 在隔离候选服务验证画布、跨图状态和关闭后的科学进程释放。
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {chromium}=require('../qa/node_modules/playwright');
const config=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const out=path.resolve(config.output);fs.mkdirSync(out,{recursive:true});
const report={platform:process.platform,checks:[],limitations:['Canvas/HTTP acceptance; does not replace real native installer or TOPCAT/Aladin interoperability tests.']};
const save=()=>fs.writeFileSync(path.join(out,'interactive-validation.json'),JSON.stringify(report,null,2));
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const context=await browser.newContext({viewport:{width:1400,height:950}}),errors=[];
 context.on('page',page=>page.on('pageerror',error=>errors.push(String(error))));
 const command=(page,operation,params={})=>page.evaluate(({operation,params})=>window.FitsViewer.command(operation,params),{operation,params});
 const state=page=>page.evaluate(()=>window.FitsViewer.state());
 async function check(name,run){try{const detail=await run();report.checks.push({name,passed:true,detail});save();}catch(error){report.checks.push({name,passed:false,error:String(error),stack:error.stack});save();throw error;}}
 async function api(operation,params={}){const response=await context.request.post(new URL('api/science',config.url).href,{headers:{'X-Fitsviewer-Token':config.token},data:{operation,params}});assert.ok(response.ok(),`${operation}: ${response.status()} ${await response.text()}`);return response.json();}
 async function open(file){const page=await context.newPage(),url=new URL(config.url);url.searchParams.set('path',file);url.searchParams.set('token',config.token);await page.goto(url.href);await page.waitForFunction(()=>window.FitsViewer?.state()?.loaded && !window.FitsViewer.state().loading,undefined,{timeout:60000});return page;}
 async function mode(page,mode){const s=await state(page);await command(page,'preferences',{...s.preferences,mode});await page.waitForFunction(mode=>window.FitsViewer.state()?.pro_initialized===(mode==='PRO'),mode);}
 async function tool(page,operation,params){await command(page,'pro.run',{operation,params});await page.waitForTimeout(180);await page.waitForFunction(()=>window.FitsViewer.state()?.pro_busy===false,undefined,{timeout:120000});const s=await state(page);assert.equal(s.pro_message,'Completed',s.pro_message);return command(page,'result');}
 let first,second;
 try{
  first=await open(config.image);
  await check('minimal_lazy_pro_and_compact_status',async()=>{const s=await state(first);assert.equal(s.mode,'Minimal');assert.equal(s.pro_initialized,false);assert.equal(s.multi,null);assert.equal(s.status_height,40);return {status_height:s.status_height,pro_initialized:s.pro_initialized};});
  await check('minimal_circle_annulus_measurement',async()=>{
   await command(first,'aperture.add',{name:'truth star',shape:{kind:'circle',center:[42.3,53.7],radius:6},background:{kind:'ring',center:[42.3,53.7],inner:9,outer:14},exclude:[]});
   await first.waitForFunction(()=>window.FitsViewer.state()?.measurements?.[0]?.net_flux>0);const s=await state(first);assert.equal(s.measurements[0].total_error,null);assert.ok(Math.abs(s.measurements[0].background_per_pixel-20)<1);return s.measurements[0];
  });
  await check('j2000_from_real_pointer',async()=>{const s=await state(first),box=await first.locator('canvas').boundingBox(),r=s.view_rect;await first.mouse.move(box.x+(r[0]+r[2])/2,box.y+(r[1]+r[3])/2);await first.waitForFunction(()=>window.FitsViewer.state()?.wcs && /J2000.*[0-9]/.test(window.FitsViewer.state()?.j2000),undefined,{timeout:45000});return (await state(first)).j2000;});
  await mode(first,'PRO');
  await check('star_detection_and_centroid',async()=>{const r=await tool(first,'detect_stars',{fwhm:3.3,threshold_sigma:8});const source=r.sources.reduce((a,b)=>Math.hypot(b.x-42.3,b.y-53.7)<Math.hypot(a.x-42.3,a.y-53.7)?b:a);assert.ok(Math.hypot(source.x-42.3,source.y-53.7)<.08);return source;});
  await check('cross_image_apertures_and_independent_views',async()=>{
   await command(first,'view',{stretch:'Sqrt',bias:.4,contrast:2,zoom:2});
   await command(first,'workspace.add',{paths:[config.second]});
   await command(first,'workspace.select',{index:1});await first.waitForFunction(file=>window.FitsViewer.state().path===file && !window.FitsViewer.state().loading,config.second);assert.equal((await state(first)).apertures.length,0);
   await command(first,'workspace.select',{index:0});await first.waitForFunction(file=>window.FitsViewer.state().path===file,config.image);const s=await state(first);assert.equal(s.apertures.length,1);assert.equal(s.stretch,'Sqrt');assert.equal(s.contrast,2);return {views:s.multi.views.length,apertures:s.apertures.length};
  });
  await check('close_active_workspace_tab',async()=>{await command(first,'workspace.close',{index:0});await first.waitForFunction(file=>window.FitsViewer.state().path===file && !window.FitsViewer.state().loading,config.second);assert.equal((await state(first)).multi.views.length,1);return {active:(await state(first)).path.split(/[\\/]/).at(-1)};});
  await check('mathtext_preview_without_tex',async()=>{const r=await tool(first,'math_preview',{expression:'$F_{\\lambda}/\\sigma$'});assert.equal(r.kind,'png');assert.equal(r.vector.kind,'svg');assert.equal(r.tex_engine_embedded,false);const response=await context.request.get(new URL('api/artifact?id='+r.artifact_id+'&token='+encodeURIComponent(config.token),config.url).href);assert.equal((await response.body()).subarray(1,4).toString(),'PNG');return {kind:r.kind,vector:r.vector.kind,tex_engine_embedded:r.tex_engine_embedded};});
  await first.screenshot({path:path.join(out,'pro.png')});
  await check('two_sessions_pro_release',async()=>{
   second=await open(config.image);await mode(second,'PRO');const before=await api('runtime_status');assert.equal(before.pro_sessions,2);
   await mode(first,'Minimal');await first.waitForTimeout(250);let between=await api('runtime_status');assert.equal(between.pro_sessions,1);assert.equal(between.worker_running,true);
   await second.evaluate(()=>window.FitsViewer.dispose());await second.close();second=null;
   let after;for(let i=0;i<80;i++){after=await api('runtime_status');if(!after.worker_running && after.pro_sessions===0)break;await first.waitForTimeout(100);}assert.equal(after.pro_sessions,0);assert.equal(after.worker_running,false);return {before,between,after};
  });
  await check('tap_upload_path_confinement',async()=>{const response=await context.request.post(new URL('api/science',config.url).href,{headers:{'X-Fitsviewer-Token':config.token},data:{operation:'vo_query',params:{protocol:'tap',uploads:{outside:config.outside||'/etc/passwd'}}}});assert.equal(response.status(),403);return {status:response.status()};});
  await check('fixed_two_rows_across_viewport_sizes',async()=>{const heights=[];for(const width of [850,1100,1400]){await first.setViewportSize({width,height:950});await first.waitForTimeout(200);const s=await state(first);heights.push(s.status_height);assert.equal(s.status_height,40);}await first.screenshot({path:path.join(out,'minimal.png')});return heights;});
  await check('closed_view_artifact_cleanup',async()=>{await first.evaluate(()=>window.FitsViewer.dispose());await first.close();first=null;let result;for(let i=0;i<60;i++){result=await api('runtime_status');if(result.artifacts===0 && !result.worker_running)break;await new Promise(resolve=>setTimeout(resolve,100));}assert.equal(result.artifacts,0);assert.equal(result.artifact_bytes,0);return result;});
  assert.deepEqual(errors,[]);report.passed=true;save();
 }finally{if(first)await first.close().catch(()=>{});if(second)await second.close().catch(()=>{});await browser.close();}
 console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,output:path.basename(out)}));
})().catch(error=>{report.passed=false;save();console.error(error);process.exitCode=1;});
