/* PRO-only merged document/comparison-row check. Configuration is private. */
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),{chromium}=require('playwright');
const cfg=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));fs.mkdirSync(cfg.output,{recursive:true});
const report={passed:false,checks:[],snapshots:[],assets:{},errors:[]},clean=s=>String(s).replace(/token=[^&\s"')]+/g,'token=[redacted]');
const save=()=>fs.writeFileSync(path.join(cfg.output,'pro-document-row-result.json'),JSON.stringify(report,null,2));
(async()=>{const browser=await chromium.launch({headless:true,args:['--no-sandbox','--disable-dev-shm-usage','--use-angle=swiftshader','--enable-unsafe-swiftshader']}),context=await browser.newContext({viewport:{width:1500,height:1000},serviceWorkers:'block'}),page=await context.newPage();let frame;
if(cfg.override_assets!==false)await context.route(/\/pkg\/fitsviewer_web(?:_bg\.wasm|\.js)(?:\?.*)?$/,async r=>{const name=new URL(r.request().url()).pathname.split('/').pop(),body=fs.readFileSync(path.join(cfg.candidate_web,'pkg',name));report.assets[name]=crypto.createHash('sha256').update(body).digest('hex');await r.fulfill({status:200,contentType:name.endsWith('.wasm')?'application/wasm':'application/javascript',body});});
page.on('pageerror',e=>report.errors.push(clean(e)));
const state=()=>frame.evaluate(()=>window.FitsViewer.state()),cmd=(operation,params={})=>frame.evaluate(({operation,params})=>window.FitsViewer.command(operation,params),{operation,params});
async function wait(file){await frame.waitForFunction(file=>{const s=window.FitsViewer?.state();return s?.loaded&&!s.loading&&s.path===file&&s.first_frame_ready},file,{timeout:60000});await page.waitForTimeout(180);}
function geometry(s){const m=s.multi_toolbar,row=m.row;assert.ok(row&&s.documents.rect);assert.deepEqual(row,s.documents.rect);assert.ok(Math.abs(row[1]-s.ribbon_rect[3])<.5);assert.ok(Math.abs(s.view_rect[1]-row[3])<.5);assert.ok(row[3]-row[1]<=32);assert.ok(m.documents[2]<=m.controls[0]);for(const name of ['add','copy','cache']){const r=m[name];assert.ok(r[0]>=m.controls[0]-.5&&r[2]<=m.controls[2]+.5,JSON.stringify({name,r,m}));assert.ok(r[1]>=row[1]&&r[3]<=row[3]);}return {row,ribbon:s.ribbon_rect,image:s.view_rect,controls:m,documents:s.documents};}
async function point(rect){const b=await frame.locator('canvas').boundingBox();return [b.x+(rect[0]+rect[2])/2,b.y+(rect[1]+rect[3])/2];}
async function clickDoc(index,close=false){
 let s=await state(),clip=s.multi_toolbar.documents,b=await frame.locator('canvas').boundingBox();await page.mouse.move(b.x+(clip[0]+clip[2])/2,b.y+(clip[1]+clip[3])/2);await page.mouse.wheel(index===0?-1600:1600,0);await page.waitForTimeout(250);
 s=await state();clip=s.multi_toolbar.documents;const r=s.documents.buttons.find(b=>b.index===index)[close?'close':'select'];const visible=[Math.max(r[0],clip[0]),Math.max(r[1],clip[1]),Math.min(r[2],clip[2]),Math.min(r[3],clip[3])];assert.ok(visible[2]-visible[0]>5,JSON.stringify({index,close,r,clip}));await page.mouse.click(...await point(visible));
}
async function task(name){const s=await state();await page.mouse.click(...await point(s.ribbon_tabs[name]));await frame.waitForFunction(name=>{const s=window.FitsViewer.state();return s.ribbon_tab===name&&!s.ribbon_collapsed},name);await page.waitForTimeout(180);}
try{
 await page.goto(cfg.base_url+'lab/workspaces/'+cfg.workspace+'/tree/'+path.posix.dirname(cfg.image1)+'?token='+encodeURIComponent(cfg.token));await page.getByText(path.basename(cfg.image1),{exact:true}).waitFor({timeout:60000});await page.getByText(path.basename(cfg.image1),{exact:true}).dblclick();for(let i=0;i<100;i++){frame=page.frames().find(f=>f.url().includes('/fitsviewer/'));if(frame)break;await page.waitForTimeout(100);}assert.ok(frame);await wait(cfg.image1);assert.equal((await state()).build_features.pro,true);
 for(const language of ['en','zh-CN'])for(const width of [450,650,1100]){
  let prefs=(await state()).preferences;prefs.mode='PRO';prefs.language=language;prefs.ribbon_tab='View';prefs.ribbon_collapsed=false;await cmd('preferences',prefs);
  const box=await frame.locator('canvas').boundingBox(),v=page.viewportSize();await page.setViewportSize({width:Math.round(v.width+width-box.width),height:1000});const b=await frame.locator('canvas').boundingBox();await page.mouse.move(b.x+b.width/2,b.y+b.height/2);await frame.waitForFunction(width=>Math.abs(window.FitsViewer.state().multi_toolbar?.row?.[2]-width)<.5,width);await page.waitForTimeout(180);
  const one=geometry(await state());await cmd('open',{path:cfg.image2,destination:'tab'});await wait(cfg.image2);const two=geometry(await state());assert.deepEqual(one.row,two.row);
  await clickDoc(0);await wait(cfg.image1);await clickDoc(1);await wait(cfg.image2);
  if(width===1100){await task('Measure');assert.deepEqual(geometry(await state()).row,two.row);await task('View');assert.deepEqual(geometry(await state()).row,two.row);}
  const name=language+'-'+width;await page.screenshot({path:path.join(cfg.output,name+'.png')});await clickDoc(1,true);await wait(cfg.image1);const closed=geometry(await state());assert.deepEqual(closed.row,one.row);
  await clickDoc(0,true);await frame.waitForFunction(()=>{const s=window.FitsViewer.state();return !s.loaded&&s.documents.views.length===0});let zero=await state();assert.equal(zero.documents.rect,null);assert.equal(zero.multi_toolbar,null);
  report.checks.push({name,passed:true});report.snapshots.push({name,one,two,closed,zero:{documents:zero.documents,loaded:zero.loaded,multi_toolbar:zero.multi_toolbar}});save();await cmd('open',{path:cfg.image1,destination:'replace'});await wait(cfg.image1);
 }
 assert.deepEqual(report.errors,[]);report.passed=true;save();
}catch(e){report.failure=clean(e);save();await page.screenshot({path:path.join(cfg.output,'failure.png')}).catch(()=>{});throw e;}
finally{if(frame)await frame.evaluate(()=>window.FitsViewer?.dispose()).catch(()=>{});await browser.close();report.browser_closed=true;save();}
console.log(JSON.stringify({passed:report.passed,checks:report.checks.length,assets:report.assets}));
})().catch(e=>{console.error(clean(e));process.exitCode=1;});

