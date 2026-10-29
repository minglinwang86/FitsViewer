/* Focused document-strip placement and stable image task-row geometry.
   Input JSON is private. Never commit credentials, endpoints or dataset paths. */
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {chromium}=require('playwright'),cfg=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
fs.mkdirSync(cfg.output,{recursive:true});
const report={passed:false,checks:[],snapshots:[],assets:{},errors:[]};
const clean=s=>String(s).replace(/token=[^&\s"')]+/g,'token=[redacted]');
const save=()=>fs.writeFileSync(path.join(cfg.output,'tab-ribbon-result.json'),JSON.stringify(report,null,2));
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']});
 const context=await browser.newContext({viewport:{width:1500,height:1000},serviceWorkers:'block'}),page=await context.newPage();let frame;
 if(cfg.override_assets!==false)await context.route(/\/pkg\/fitsviewer_web(?:_bg\.wasm|\.js)(?:\?.*)?$/,async route=>{
  const name=new URL(route.request().url()).pathname.split('/').pop(),body=fs.readFileSync(path.join(cfg.candidate_web,'pkg',name));
  report.assets[name]=crypto.createHash('sha256').update(body).digest('hex');
  await route.fulfill({status:200,contentType:name.endsWith('.wasm')?'application/wasm':'application/javascript',body});
 });
 page.on('pageerror',e=>report.errors.push(clean(e)));
 const state=()=>frame.evaluate(()=>window.FitsViewer.state());
 const command=(operation,params={})=>frame.evaluate(({operation,params})=>window.FitsViewer.command(operation,params),{operation,params});
 const geometry=s=>({ribbon:s.ribbon_rect,task:s.ribbon_task_rect,layout:s.ribbon_layout,documents:s.documents.rect,image:s.view_rect,tab:s.ribbon_tab,collapsed:s.ribbon_collapsed,count:s.documents.views.length});
 function placement(s){const g=geometry(s);assert.ok(g.documents,'Missing document row');assert.ok(g.ribbon[3]<=g.documents[1]+.5,JSON.stringify(g));assert.ok(g.documents[3]<=g.image[1]+.5,JSON.stringify(g));assert.ok(g.image[1]-g.documents[3]<18,JSON.stringify(g));return g;}
 function same(a,b){for(const [name,indices]of [['ribbon',[1,3]],['documents',[1,3]],['image',[1]]])for(const i of indices)assert.ok(Math.abs(a[name][i]-b[name][i])<=.5,JSON.stringify({a,b}));}
 async function waitImage(file){await frame.waitForFunction(file=>{const s=window.FitsViewer?.state();return s?.loaded&&!s.loading&&s.path===file&&s.first_frame_ready},file,{timeout:60000});await page.waitForTimeout(180);}
 async function clickTask(tab){
  const s=await state(),r=s.ribbon_tabs[tab],box=await frame.locator('canvas').boundingBox();assert.ok(r&&box);
  await frame.evaluate(()=>{window.__layoutSamples=[];});
  await page.mouse.click(box.x+(r[0]+r[2])/2,box.y+(r[1]+r[3])/2);
  await frame.waitForFunction(tab=>window.__layoutSamples?.some(s=>s.tab===tab),tab);
  const first=await frame.evaluate(tab=>window.__layoutSamples.find(s=>s.tab===tab),tab);
  assert.equal(first.collapsed,false,'Activating another task collapsed its body');
  await page.waitForTimeout(180);return {first,settled:placement(await state())};
 }
 try{
  await page.goto(cfg.base_url+'lab/workspaces/'+cfg.workspace+'/tree/'+path.posix.dirname(cfg.image1)+'?token='+encodeURIComponent(cfg.token));
  await page.getByText(path.basename(cfg.image1),{exact:true}).waitFor({timeout:60000});
  await page.getByText(path.basename(cfg.image1),{exact:true}).dblclick();
  for(let i=0;i<100;i++){frame=page.frames().find(f=>f.url().includes('/fitsviewer/'));if(frame)break;await page.waitForTimeout(100);}
  assert.ok(frame);await waitImage(cfg.image1);assert.equal((await state()).build_features.pro,cfg.pro);
  await frame.evaluate(()=>{
   let current=window.__fitsviewerState;window.__layoutSamples=[];
   Object.defineProperty(window,'__fitsviewerState',{configurable:true,get:()=>current,set:value=>{
    current=value;if(value?.ribbon_rect&&value.documents?.rect)window.__layoutSamples.push({
     ribbon:value.ribbon_rect,task:value.ribbon_task_rect,layout:value.ribbon_layout,documents:value.documents.rect,image:value.view_rect,tab:value.ribbon_tab,collapsed:value.ribbon_collapsed,count:value.documents.views.length});
    if(window.__layoutSamples.length>100)window.__layoutSamples.shift();
   }});
  });
  for(const language of ['en','zh-CN'])for(const mode of cfg.pro?['Minimal','PRO']:['Minimal'])for(const width of cfg.widths||[650,1100,1600]){
   let prefs=(await state()).preferences;prefs.language=language;prefs.mode=mode;prefs.ribbon_tab='View';prefs.ribbon_collapsed=false;await command('preferences',prefs);
   const box=await frame.locator('canvas').boundingBox(),viewport=page.viewportSize();
   await page.setViewportSize({width:Math.round(viewport.width+width-box.width),height:1000});
   const resized=await frame.locator('canvas').boundingBox();await page.mouse.move(resized.x+resized.width/2,resized.y+resized.height/2);
   await frame.waitForFunction(width=>{const r=window.FitsViewer.state().documents?.rect;return r&&Math.abs(r[2]-r[0]-width)<.5},width);
   await page.waitForTimeout(180);
   const one=placement(await state());assert.equal(one.count,1);
   await command('open',{path:cfg.image2,destination:'tab'});await waitImage(cfg.image2);const two=placement(await state());assert.equal(two.count,2);same(one,two);
   const rows=[];for(const tab of ['Measure','Coordinates','View']){const row=await clickTask(tab);same(two,row.first);same(two,row.settled);rows.push({tab,...row});}
   if(width===1100){
    const s=await state(),r=s.ribbon_tabs.View,box=await frame.locator('canvas').boundingBox();
    // Separate this gesture from the preceding rapid switch, then deliver two real frames.
    await page.waitForTimeout(450);
    await page.mouse.dblclick(box.x+(r[0]+r[2])/2,box.y+(r[1]+r[3])/2,{delay:80});
    await frame.waitForFunction(()=>window.FitsViewer.state().ribbon_collapsed);
    const restore=(await state()).preferences;restore.ribbon_collapsed=false;await command('preferences',restore);
    await frame.waitForFunction(()=>!window.FitsViewer.state().ribbon_collapsed);await page.waitForTimeout(180);same(two,placement(await state()));
   }
   await command('tab.close',{index:1});await waitImage(cfg.image1);const closed=placement(await state());assert.equal(closed.count,1);same(one,closed);
   const name=language+'-'+mode+'-'+width;report.checks.push({name,passed:true});report.snapshots.push({name,one,two,closed,switches:rows});save();
   if(width===1100)await page.screenshot({path:path.join(cfg.output,name+'.png')});
  }
  assert.deepEqual(report.errors,[]);report.passed=true;save();
 }catch(error){report.failure=clean(error);save();await page.screenshot({path:path.join(cfg.output,'failure.png')}).catch(()=>{});throw error;}
 finally{if(frame)await frame.evaluate(()=>window.FitsViewer?.dispose()).catch(()=>{});await browser.close();report.browser_closed=true;save();}
 console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,assets:report.assets}));
})().catch(error=>{console.error(clean(error));process.exitCode=1;});

