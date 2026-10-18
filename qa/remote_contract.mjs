// Mock fetch contracts only: no browser or user service is started.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const source=await readFile(new URL('../web/remote.js',import.meta.url),'utf8');
const {imageFetchBoundary,remoteSession}=await import('data:text/javascript;base64,'+Buffer.from(source).toString('base64'));
const calls=[];
const original=(input,options)=>{const call={url:String(input),options};calls.push(call);return new Promise((resolve,reject)=>{call.resolve=resolve;options.signal?.addEventListener('abort',()=>reject(new DOMException('Aborted','AbortError')),{once:true});if(options.signal?.aborted)reject(new DOMException('Aborted','AbortError'));});};
const boundary=imageFetchBoundary('http://127.0.0.1:8877/',()=> 'fv1_fixture',original);
const a=boundary.fetch('http://127.0.0.1:8877/api/image/tile?fv_image=one&level=0&x=1&y=2').catch(error=>error.name);
const b=boundary.fetch('http://127.0.0.1:8877/api/image/tile?fv_image=one&level=0&x=3&y=2');
const other=boundary.fetch('http://127.0.0.1:8877/api/image/tile?fv_image=other&level=0&x=1&y=2').catch(error=>error.name);
assert.equal(calls[0].options.headers.get('X-Fitsviewer-Token'),'fv1_fixture');
boundary.keep('one',['0:3:2']);
assert.equal(await a,'AbortError');assert.equal(calls[1].options.signal.aborted,false);assert.equal(calls[2].options.signal.aborted,false);
calls[1].resolve(new Response('tile bytes'));assert.equal(await (await b).text(),'tile bytes');
boundary.keep('other',[]);assert.equal(await other,'AbortError');
const headersOnly=boundary.fetch('http://127.0.0.1:8877/api/image/full?fv_image=body');
let bodyController;
calls.at(-1).resolve(new Response(new ReadableStream({start(controller){bodyController=controller;}})));
const bodyResponse=await headersOnly;
// Controller must remain registered after response headers; a group close still cancels it.
boundary.keep('body',[]);assert.equal(calls.at(-1).options.signal.aborted,true);bodyController.close();await bodyResponse.text();
const plain=boundary.fetch('http://127.0.0.1:8877/fonts/cjk.ttf');assert.equal(calls.at(-1).options.signal,undefined);assert.equal(calls.at(-1).options.headers,undefined);calls.at(-1).resolve(new Response('font'));await plain;
const external=boundary.fetch('https://untrusted.example/api/image/tile');assert.equal(calls.at(-1).options.signal,undefined);assert.equal(calls.at(-1).options.headers,undefined);calls.at(-1).resolve(new Response('external'));await external;
const artifact=boundary.fetch('http://127.0.0.1:8877/api/artifact?id=fixture&token=must-not-travel');assert.equal(new URL(calls.at(-1).url).searchParams.has('token'),false);assert.equal(calls.at(-1).options.signal,undefined);calls.at(-1).resolve(new Response('export'));await artifact;
const requestCalls=[];
const requestBoundary=imageFetchBoundary('http://127.0.0.1:8877/',()=> 'fv1_fixture',async(input,options)=>{
 requestCalls.push({input,options});return new Response('ok');
});
const post=new Request('http://127.0.0.1:8877/api/science',{method:'POST',headers:{'Content-Type':'application/json'},body:new TextEncoder().encode('{"operation":"header"}')});
await requestBoundary.fetch(post);
assert.equal(requestCalls[0].input,post);assert.equal(requestCalls[0].options.body,undefined);
assert.equal(await post.clone().text(),'{"operation":"header"}');assert.equal(requestCalls[0].options.headers.get('X-Fitsviewer-Token'),'fv1_fixture');
const rewrittenPost=new Request('http://127.0.0.1:8877/api/science?token=legacy-secret',{method:'POST',headers:{'Content-Type':'application/json'},body:new TextEncoder().encode('{"operation":"header"}')});
await requestBoundary.fetch(rewrittenPost);
assert.equal(new URL(requestCalls[1].input).searchParams.has('token'),false);assert.ok(requestCalls[1].options.body instanceof Uint8Array);
assert.equal(new TextDecoder().decode(requestCalls[1].options.body),'{"operation":"header"}');assert.equal(requestCalls[1].options.method,'POST');
await assert.rejects(requestBoundary.fetch(new Request('http://127.0.0.1:8877/api/science?token=legacy-secret',{method:'POST',body:new Uint8Array(8*1024*1024+1)})),/8 MiB/);
assert.equal(requestCalls.length,2);requestBoundary.dispose();
boundary.dispose();
const oldSetTimeout=globalThis.setTimeout,oldClearTimeout=globalThis.clearTimeout,oldFetch=globalThis.fetch;
const timers=new Map();let timerId=0;
globalThis.setTimeout=(callback,ms)=>{const id=++timerId;timers.set(id,{callback,ms});return id;};
globalThis.clearTimeout=id=>timers.delete(id);
try {
 const timed=imageFetchBoundary('http://127.0.0.1:8877/',()=>'',original);
 const metadata=timed.fetch('http://127.0.0.1:8877/api/image/metadata?path=fixture').catch(error=>error.name);
 const deadline=[...timers.values()].find(timer=>timer.ms===60000);assert.ok(deadline);deadline.callback();assert.equal(await metadata,'AbortError');
 const pixel=timed.fetch('http://127.0.0.1:8877/api/image/pixel?path=fixture').catch(error=>error.name);
 const short=[...timers.values()].find(timer=>timer.ms===20000);assert.ok(short);short.callback();assert.equal(await pixel,'AbortError');timed.dispose();
 let request={request_id:'fixture',attempt_epoch:1,seq:1,path:'/synthetic.fits',hdu:null,route:'browser',state:'queued'};
 const actions=[],urls=[];let started=false,state={path:'/synthetic.fits',first_frame_ready:false,loading:true},opens=0,fallback;
 globalThis.fetch=async(url,options)=>{
  urls.push(String(url));const route=new URL(url).pathname.split('/').at(-1),data=options.body?JSON.parse(options.body):{};
  let value={instance:'fixture-instance',target:'fixture-target'};
  if(route==='bootstrap')value={...value,token:'fv1_session-fixture',fixed:false,request:{...request}};
  else if(route==='events')value={...value,requests:[{...request}]};
  else if(route==='ack'){
   actions.push(data.action);
   assert.equal(options.headers['X-Fitsviewer-Token'],'fv1_session-fixture');
   if(data.action==='fallback'){request.route='browser';request.state='queued';request.attempt_epoch++;}
   if(data.action==='claim')request.state='accepted';if(data.action==='visible')request.state='visible';if(data.action==='first_frame')request.state='first_frame';
   value={...value,request:{...request}};
  }
  return new Response(JSON.stringify(value),{headers:{'Content-Type':'application/json'}});
 };
 const ui={started:()=>started,state:()=>state,open:async()=>{opens++;},close:async()=>{},status(text,callback){fallback=callback;}};
 const session=remoteSession('http://127.0.0.1:8877/','one-time-fixture',ui);
 await session.bootstrap();assert.deepEqual(actions,['claim','visible']);assert.equal(opens,0);
 started=true;session.follow();await new Promise(setImmediate);assert.equal(actions.includes('first_frame'),false);
 state.first_frame_ready=true;const next=[...timers.entries()].find(([,timer])=>timer.ms===1000);assert.ok(next);timers.delete(next[0]);await next[1].callback();assert.equal(actions.filter(action=>action==='first_frame').length,1);
 assert.ok(urls.every(url=>!url.includes('fv1_session-fixture')));session.dispose();await new Promise(setImmediate);
 timers.clear();actions.length=0;opens=0;request={...request,request_id:'helper-fixture',attempt_epoch:1,route:'helper',state:'unknown'};
 const helper=remoteSession('http://127.0.0.1:8877/','helper-link-fixture',ui);await helper.bootstrap();helper.follow();await new Promise(setImmediate);
 assert.equal(opens,0);assert.equal(actions.includes('claim'),false);assert.equal(typeof fallback,'function');
 await fallback();assert.equal(actions.filter(action=>action==='fallback').length,1);assert.equal(actions.filter(action=>action==='claim').length,1);assert.equal(opens,1);helper.dispose();await new Promise(setImmediate);
}finally{globalThis.setTimeout=oldSetTimeout;globalThis.clearTimeout=oldClearTimeout;globalThis.fetch=oldFetch;}
console.log(JSON.stringify({ok:true,checks:['group-specific cancellation','tile key retention','body remains cancellable after headers','close group','font and foreign URL untouched','artifact query token stripped','real Request POST keeps internal byte body','rewritten Request POST buffers bytes with 8 MiB limit','metadata/full and tile/pixel deadlines','claim is distinct from first frame','unknown helper never blindly relaunched','explicit browser takeover bumps attempt'],browser_started:false}));
