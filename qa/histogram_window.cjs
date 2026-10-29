/* Focused browser-only histogram regression. Input is a private JSON file; never log credentials. */
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const {chromium}=require('playwright');
// Input JSON: base_url, token, workspace, output, image1, image2, candidate_web.
// Use two synthetic FITS files under an isolated Jupyter root/workspace.
// candidate_web supplies expected hashes; override_assets=true routes these bytes
// only in this QA browser. Default false checks the installed server's assets.
// X/close/reopen must preserve geometry as well as the count-scale preference.

assert.ok(process.argv[2],'Pass a private QA JSON configuration file');
const cfg=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const out=path.resolve(cfg.output);fs.mkdirSync(out,{recursive:true});
const sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
const assets={};for(const file of ['fitsviewer_web.js','fitsviewer_web_bg.wasm']){const body=fs.readFileSync(path.join(cfg.candidate_web,'pkg',file));assets[file]={body,sha256:sha(body)};}
const report={schema:1,passed:false,checks:[],snapshots:[],candidate_assets:Object.fromEntries(Object.entries(assets).map(([k,v])=>[k,v.sha256])),served_assets:[],errors:[],method:cfg.override_assets?'Private browser context asset routes; user installation and tabs untouched':'Installed-server asset validation; isolated browser context and workspace'};
const save=()=>fs.writeFileSync(path.join(out,'histogram-fixed-result.json'),JSON.stringify(report,null,2));
(async()=>{
 const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader','--renderer-process-limit=1']});
 const context=await browser.newContext({viewport:{width:1600,height:1050},serviceWorkers:'block'}),page=await context.newPage();
 const responses=[];let frame;
 if(cfg.override_assets)await context.route(/\/pkg\/fitsviewer_web(?:_bg\.wasm|\.js)(?:\?.*)?$/,async route=>{
  const name=new URL(route.request().url()).pathname.split('/').pop(),asset=assets[name];
  assert.ok(asset,'Unexpected generated module request');
  await route.fulfill({status:200,contentType:name.endsWith('.wasm')?'application/wasm':'application/javascript',headers:{'Cache-Control':'no-store'},body:asset.body});
 });
 page.on('pageerror',e=>report.errors.push(String(e).replace(/token=[^&\s"')]+/g,'token=[redacted]')));
 page.on('response',r=>{if(/\/pkg\/fitsviewer_web(?:_bg\.wasm|\.js)(?:\?.*)?$/.test(r.url()))responses.push((async()=>{
  const name=new URL(r.url()).pathname.split('/').pop(),status=r.status();
  // Redirect/cache-validation responses have no response body. The terminal 200
  // response is still required and hash-checked for each generated asset below.
  if(status>=300&&status<400){(report.asset_redirects_or_cache_validation??=[]).push({file:name,status});return;}
  assert.equal(status,200,'Unexpected generated-asset HTTP status');
  const actual=sha(await r.body());report.served_assets.push({file:name,sha256:actual,source:cfg.override_assets?'qa-route':'server'});assert.equal(actual,assets[name].sha256);
 })().catch(error=>{report.errors.push(String(error).replace(/token=[^&\s"')]+/g,'token=[redacted]'));save();}));});
 const command=(op,params)=>frame.evaluate(({op,params})=>window.FitsViewer.command(op,params),{op,params});
 const state=()=>frame.evaluate(()=>window.FitsViewer.state());
 async function settle(){await page.waitForTimeout(400);}
 async function selectFrame(){
  await page.waitForFunction(file=>[...document.querySelectorAll('iframe[data-testid="fitsviewer-frame"]')].some(e=>{const p=new URL(e.src).searchParams.get('path');return p===file||p?.endsWith('/'+file);}),path.basename(cfg.image1),{timeout:60000});
  for(let attempt=0;attempt<100;attempt++){frame=page.frames().find(f=>{try{const p=new URL(f.url()).searchParams.get('path'),file=path.basename(cfg.image1);return p===file||p?.endsWith('/'+file);}catch{return false;}});if(frame)break;await page.waitForTimeout(100);}assert.ok(frame,'Viewer iframe did not navigate');
  await frame.waitForFunction(()=>window.FitsViewer?.state()?.loaded&&window.FitsViewer.state().first_frame_ready,undefined,{timeout:90000});await settle();
 }
 async function snap(name){const s=await state();const geometry=await frame.evaluate(()=>({canvas:[document.querySelector('canvas').width,document.querySelector('canvas').height],viewport:[innerWidth,innerHeight],log_stored:JSON.parse(localStorage.getItem('fitsviewer-ui-v1')||'{}').histogram_log_counts}));const identity=await frame.evaluate(()=>window.__histogramQaAppInstance||(window.__histogramQaAppInstance=crypto.randomUUID()));const row={name,...geometry,app_instance:identity,loaded:s.loaded,first_frame_ready:s.first_frame_ready,document_generation:s.generation,mode:s.mode,prefs_context:s.spectrum?'spectrum':'image',histogram:s.preferences.window_layouts.image.Histogram,log_counts:s.preferences.histogram_log_counts};report.snapshots.push(row);save();if(await frame.locator('canvas').boundingBox())await page.screenshot({path:path.join(out,name+'.png')});return row;}
 function sameGeometry(a,b){for(const key of ['rect','size']){assert.equal(a.histogram[key].length,b.histogram[key].length);a.histogram[key].forEach((v,i)=>assert.ok(Math.abs(v-b.histogram[key][i])<=.5,`${key}[${i}]: ${v} -> ${b.histogram[key][i]}`));}}
 async function check(name,run){await run();report.checks.push({name,passed:true});save();}
 async function drag(start,end){const b=await frame.locator('canvas').boundingBox();await page.mouse.move(b.x+start[0],b.y+start[1]);await page.mouse.down();await page.mouse.move(b.x+end[0],b.y+end[1],{steps:16});await page.mouse.up();await settle();}
 async function click(x,y){const b=await frame.locator('canvas').boundingBox();await page.mouse.click(b.x+x,b.y+y);await settle();}
 try{
  const directory=path.posix.dirname(cfg.image1);
  const tree=directory==='.'?'':'/tree/'+directory.split('/').map(encodeURIComponent).join('/');
  await page.goto(cfg.base_url+'lab/workspaces/'+encodeURIComponent(cfg.workspace)+tree+'?token='+encodeURIComponent(cfg.token));
  await page.getByText(path.basename(cfg.image1),{exact:true}).waitFor({timeout:60000});await page.getByText(path.basename(cfg.image1),{exact:true}).dblclick();await selectFrame();
  const initial=await state();assert.ok(Object.hasOwn(initial.preferences,'histogram_log_counts'),'New preference field absent: wrong WASM');
  if(cfg.language){const preferences=initial.preferences;preferences.language=cfg.language;await command('preferences',preferences);await frame.waitForFunction(language=>window.FitsViewer.state().preferences.language===language,cfg.language);await settle();}
  await command('histogram',{open:true});await settle();let s=await snap('01-default-expanded');
  await check('compact_expanded_histogram',async()=>{assert.ok(s.histogram.size[0]>=300&&s.histogram.size[0]<=335&&s.histogram.size[1]>=96&&s.histogram.size[1]<=160);const r=s.histogram.rect,aspect=(r[2]-r[0])/(r[3]-r[1]);assert.ok(aspect>=1.65&&aspect<=1.95,'Default outer aspect should be close to 16:9: '+aspect);assert.equal(s.histogram.collapsed,false);});
  await check('title_controls_and_details_menu',async()=>{
   const st=await state(),box=st.preferences.window_layouts.image.Histogram.rect,c=st.histogram_controls;
   assert.ok(c?.log_counts_rect&&c?.details_rect&&c?.plot_rect,'New title controls absent');
   assert.ok(c.log_counts_rect[1]>=box[1]&&c.log_counts_rect[3]<=box[1]+32,'Log counts must be in the title row');
   await click((c.details_rect[0]+c.details_rect[2])/2,(c.details_rect[1]+c.details_rect[3])/2);
   await frame.waitForFunction(()=>window.FitsViewer.state().histogram_controls.details_open);
   await snap('02-details-menu');await page.keyboard.press('Escape');await settle();
  });
  s=await state();let r=s.preferences.window_layouts.image.Histogram.rect;await drag([r[0]+80,r[1]+12],[460,340]);r=(await state()).preferences.window_layouts.image.Histogram.rect;const beforeResize=await state();await drag([r[2]-3,r[3]-3],[r[2]+160,r[3]+130]);
  await check('histogram_window_and_plot_resize',async()=>{const enlarged=await state();assert.ok(enlarged.preferences.window_layouts.image.Histogram.size[1]>beforeResize.preferences.window_layouts.image.Histogram.size[1]+40,'Window must grow');assert.ok(enlarged.histogram_controls.plot_rect[3]-enlarged.histogram_controls.plot_rect[1]>beforeResize.histogram_controls.plot_rect[3]-beforeResize.histogram_controls.plot_rect[1]+30,'Plot must grow with its window');const q=enlarged.preferences.window_layouts.image.Histogram.rect;await drag([q[2]-3,q[3]-3],[q[2]-100,q[1]+140]);const shrunk=await state();assert.ok(shrunk.preferences.window_layouts.image.Histogram.size[1]<enlarged.preferences.window_layouts.image.Histogram.size[1]-30,'Window must also shrink');});
  const baseline=await snap('03-moved-resized');
  await check('jupyter_tab_hide_restore_geometry',async()=>{
   await page.getByText(path.basename(cfg.image2),{exact:true}).dblclick();await page.waitForFunction(()=>document.querySelectorAll('iframe[data-testid="fitsviewer-frame"]').length===2);await page.waitForTimeout(1000);const hidden=await snap('04-jupyter-hidden');assert.deepEqual(hidden.canvas,[0,0]);sameGeometry(baseline,hidden);
   await page.locator('.lm-TabBar-tab').filter({hasText:path.basename(cfg.image1)}).click();await settle();sameGeometry(baseline,await snap('05-jupyter-restored'));
  });
  await check('explicit_display_none_restore_geometry',async()=>{
   await frame.frameElement().then(el=>el.evaluate(el=>{el.dataset.qaDisplay=el.style.display;el.style.display='none';}));await page.waitForTimeout(1000);const hidden=await snap('06-display-none');assert.deepEqual(hidden.canvas,[0,0]);sameGeometry(baseline,hidden);
   await frame.frameElement().then(el=>el.evaluate(el=>{el.style.display=el.dataset.qaDisplay;delete el.dataset.qaDisplay;}));await settle();sameGeometry(baseline,await snap('07-display-restored'));
  });
  for(const expected of [true,false])await check('log_counts_'+expected+'_persists',async()=>{
   let s=await state();if(s.preferences.histogram_log_counts!==expected){const r=s.histogram_controls.log_counts_rect;await click((r[0]+r[2])/2,(r[1]+r[3])/2);}
   await frame.waitForFunction(expected=>window.FitsViewer.state().preferences.histogram_log_counts===expected,expected,{timeout:10000});
   let captured=await snap('10-log-'+expected+'-clicked');assert.equal(captured.log_stored,expected);
   let r=captured.histogram.rect;await click(r[2]-11,r[1]+12);await frame.waitForFunction(()=>!window.FitsViewer.state().preferences.window_layouts.image.Histogram.open);await snap('10-log-'+expected+'-closed');
   await command('histogram',{open:true});await settle();assert.equal((await state()).preferences.histogram_log_counts,expected);const reopened=await snap('11-log-'+expected+'-reopened');assert.equal(reopened.app_instance,captured.app_instance);assert.equal(reopened.first_frame_ready,true);assert.equal(reopened.prefs_context,captured.prefs_context);sameGeometry(captured,reopened);
   await page.reload();await selectFrame();await page.locator('.lm-TabBar-tab').filter({hasText:path.basename(cfg.image1)}).click();await settle();if(!(await state()).preferences.window_layouts.image.Histogram.open){await command('histogram',{open:true});await settle();}
   captured=await snap('12-log-'+expected+'-reloaded');assert.equal(captured.log_counts,expected);assert.equal(captured.log_stored,expected);
  });
  await Promise.all(responses);for(const name of Object.keys(assets))assert.ok(report.served_assets.some(x=>x.file===name));
  assert.deepEqual(report.errors,[]);report.passed=true;save();
 }catch(e){report.failure=String(e).replace(/token=[^&\s"')]+/g,'token=[redacted]');save();throw e;}
 finally{for(const f of page.frames())if(f.url().includes('/fitsviewer/'))await f.evaluate(()=>window.FitsViewer?.dispose()).catch(()=>{});await browser.close();report.test_browser_closed=true;save();}
 console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,candidate_assets:report.candidate_assets}));
})().catch(e=>{console.error(String(e).replace(/token=[^&\s"')]+/g,'token=[redacted]'));process.exitCode=1;});

