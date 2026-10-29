// Execute the actual page bridge with a controlled renderer/API, on Linux.
import fs from 'node:fs';
import vm from 'node:vm';
import assert from 'node:assert/strict';
const html=fs.readFileSync(new URL('../web/index.html',import.meta.url),'utf8');
const bridge=html.slice(html.indexOf('// Selection sessions keep'),html.indexOf('function releaseResources(){'));
function fixture(){
 const requests=[],commands=[],timers=[];
 const selection={id:'owned',revision:2,status:'selecting',points:[[12.25,33.75]],source:{path:'a.fits',hdu:0,width:100,height:80,identity:'version-1'},applied_config_revision:1};
 const state={selection,loaded:true,loading:false,path:'a.fits',hdu:0,science_session:'viewer-owned',low:1,high:20};
 let session={id:'owned',status:'selecting',path:'a.fits',hdu:0,max_points:2,identity:'version-1',config_revision:1,requested_view:{},view:{},prompt:'Pick a star',markers:[]};
 const context=vm.createContext({URL,URLSearchParams,console,disposed:false,remote:null,base:'http://127.0.0.1:12345/',document:{cookie:'_xsrf=test-xsrf'},Date,
   setTimeout(callback){timers.push(callback);return timers.length;},fetch:async(url,options)=>{requests.push({url:String(url),options});if(context.failure)return {ok:false,status:context.failure,text:async()=> 'Session is already used'};return {ok:true,json:async()=> structuredClone(session)};},
   window:{__fitsviewerState:state,FitsViewer:{async command(operation,params){commands.push({operation,params});if(operation==='open'){state.path=params.path;state.hdu=params.hdu;state.loaded=true;state.loading=false;}if(operation==='selection.configure'){state.selection.applied_config_revision=params.config_revision;state.selection.revision++;}if(operation==='view'){Object.assign(state,params);}if(operation==='selection.cancel')state.selection.status='cancelled';}}},failure:0});
 vm.runInContext(bridge+'\nglobalThis.control={beginSelection,followSelection,selectionUpdate,cancelSelectionOnClose,applySelectionConfiguration,setSession(id){selectionSession=id;},getSession(){return selectionSession;}}',context);
 return {context,requests,commands,timers,state,set session(value){session=value;},get session(){return session;},control:context.control};
}
{
 const f=fixture();await f.control.beginSelection('owned');
 // beginSelection starts one async follow; let its own microtask finish.
 await new Promise(resolve=>setImmediate(resolve));
 const begin=f.commands.find(c=>c.operation==='selection.begin');assert.equal(begin.params.identity,'version-1');
 const update=f.requests.filter(r=>r.options?.method==='POST').map(r=>JSON.parse(r.options.body)).at(-1);
 assert.equal(update.viewer_id,'viewer-owned');assert.deepEqual(update.points,[[12.25,33.75]]);assert.deepEqual(update.view,{low:1,high:20});assert.equal(update.applied_config_revision,1);
 assert.equal(f.requests.find(r=>r.options?.method==='POST').options.headers['X-XSRFToken'],'test-xsrf');
}
{
 const f=fixture();f.session={...f.session,config_revision:2,requested_view:{stretch:'Linear',low:3,high:9,fit:true,cmap:'grey'},view:{low:-100,high:-50}};
 await f.control.applySelectionConfiguration(f.session);
 assert.deepEqual(JSON.parse(JSON.stringify(f.commands.find(c=>c.operation==='view').params)),{stretch:'Linear',low:3,high:9});
 assert.equal(f.commands.at(-1).params.config_revision,2);assert.equal(f.commands.at(-1).params.cmap,'grey');assert.equal(f.state.low,3);
 const count=f.commands.length;await f.control.applySelectionConfiguration(f.session);assert.equal(f.commands.length,count);
}
{
 const f=fixture();f.control.setSession('owned');f.session={...f.session,status:'cancelled'};await f.control.followSelection();
 assert.equal(f.requests.length,1,'Read host terminal status before uploading stale points');assert.equal(f.requests[0].options?.method,undefined);
 assert.equal(f.state.selection.status,'cancelled');assert.equal(f.control.getSession(),null);
}
{
 const f=fixture();f.control.setSession('owned');f.context.failure=409;await f.control.followSelection();
 assert.equal(f.commands.at(-1).operation,'selection.fail');assert.equal(f.control.getSession(),null);assert.equal(f.timers.length,0,'Writer conflicts must stop retries');
}
{
 const f=fixture();f.control.setSession('owned');f.control.cancelSelectionOnClose();
 const request=f.requests[0],body=JSON.parse(request.options.body);assert.equal(body.status,'cancelled');assert.equal(body.revision,3);assert.equal(body.viewer_id,'viewer-owned');assert.equal(request.options.keepalive,true);
}
{
 const classic=html.match(/<script>([\s\S]*?)<\/script>/)[1];const location=new URL('http://127.0.0.1:12345/?selection=owned#fv_token=owned-fixture-token');const edits=[];
 const context={URLSearchParams,location,window:{},history:{replaceState(_a,_b,url){edits.push(url);}}};vm.runInNewContext(classic,context);
 assert.equal(context.window.__fvBootstrapToken,'owned-fixture-token');assert.deepEqual(edits,['/?selection=owned']);
 assert.ok(html.includes("delete window.__fvBootstrapToken"));
}
{
 const f=fixture();f.state.path='reference.fits';f.state.preferences={mode:'PRO',pro_open_destination:'layer'};
 await f.control.beginSelection('owned');const open=f.commands.find(c=>c.operation==='open');assert.equal(open.params.destination,'replace');assert.equal(open.params.path,'a.fits');assert.equal(open.params.hdu,0);
 assert.ok(f.commands.find(c=>c.operation==='selection.begin'));
}
console.log('Selection bridge: bootstrap credential, source gate, writer identity, actual view acknowledgment, terminal cancellation, conflict stop and close keepalive passed (7 scenarios).');
