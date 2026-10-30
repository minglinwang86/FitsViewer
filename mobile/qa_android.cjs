// Hosted Android emulator check: native document picker -> local WASM -> texture.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const {execFileSync}=require('node:child_process');
const apk=path.resolve(process.argv[2]),out=path.resolve(process.argv[3]);fs.mkdirSync(out,{recursive:true});
const adb=(...args)=>execFileSync('adb',args,{encoding:'utf8'}).trim(),delay=ms=>new Promise(r=>setTimeout(r,ms));
async function until(fn,seconds=60){const deadline=Date.now()+seconds*1000;let last;while(Date.now()<deadline){try{const value=await fn();if(value)return value;}catch(error){if(error.fatal)throw error;last=error;}await delay(300);}throw last||new Error('Emulator wait expired');}
function screenshot(name){fs.writeFileSync(path.join(out,name),execFileSync('adb',['exec-out','screencap','-p']));}
function dump(){adb('shell','uiautomator','dump','/sdcard/mobile-window.xml');return adb('shell','cat','/sdcard/mobile-window.xml');}
function nodes(xml){return [...xml.matchAll(/<node\s+([^>]+)>?/g)].map(node=>Object.fromEntries([...node[1].matchAll(/([\w-]+)="([^"]*)"/g)].map(m=>[m[1],m[2]])));}
function tapBounds(attributes){const b=(attributes.bounds||'').match(/\[(\d+),(\d+)\]\[(\d+),(\d+)\]/);if(!b)return false;adb('shell','input','tap',String((+b[1]+ +b[3])/2),String((+b[2]+ +b[4])/2));return true;}
function tapNode(xml,pattern){for(const attributes of nodes(xml))if(pattern.test((attributes.text||'')+' '+(attributes['content-desc']||''))&&tapBounds(attributes))return true;return false;}
(async()=>{
 const card=(k,v)=>(k.padEnd(8)+(v===undefined?'':'= '+v)).padEnd(80);
 const header=[card('SIMPLE','                    T'),card('BITPIX','                   16'),card('NAXIS','                    2'),card('NAXIS1','                   64'),card('NAXIS2','                   64'),card('END')].join('').padEnd(2880);
 const pixels=Buffer.alloc(8640);for(let i=0;i<4096;i++)pixels.writeInt16BE(i,i*2);
 fs.writeFileSync(out+'/mobile-fixture.fits',Buffer.concat([Buffer.from(header),pixels]));
 adb('install','-r',apk);
 const storageSetup={attempts:0,errors:[]};
 await until(()=>{
  storageSetup.attempts++;
  try{
   adb('shell','mkdir','-p','/sdcard/Download');
   adb('push',out+'/mobile-fixture.fits','/sdcard/Download/mobile-fixture.fits');
   if(adb('shell','stat','-c','%s','/sdcard/Download/mobile-fixture.fits')!=='11520')throw new Error('Fixture size differs after native storage copy');
   return true;
  }catch(error){storageSetup.errors.push({attempt:storageSetup.attempts,stderr:String(error.stderr||error.message)});throw error;}
  finally{fs.writeFileSync(out+'/storage-setup.json',JSON.stringify(storageSetup,null,2));}
 },60);
 adb('shell','am','start','-W','-n','org.fitsviewer.mini.test/org.fitsviewer.mini.MainActivity');
 const pid=await until(()=>adb('shell','pidof','org.fitsviewer.mini.test'));
 adb('forward','tcp:19227','localabstract:webview_devtools_remote_'+pid);
 const target=await until(async()=>{const list=await(await fetch('http://127.0.0.1:19227/json/list')).json();return list.find(t=>t.type==='page'&&t.webSocketDebuggerUrl);});
 const socket=new WebSocket(target.webSocketDebuggerUrl),pending=new Map();let serial=0;
 await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
 socket.addEventListener('message',event=>{const message=JSON.parse(event.data);if(!message.id)return;const p=pending.get(message.id);if(!p)return;pending.delete(message.id);message.error?p.reject(new Error(JSON.stringify(message.error))):p.resolve(message.result);});
 const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++serial;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));setTimeout(()=>{if(pending.delete(id))reject(new Error('CDP timeout: '+method));},20000).unref();});
 const evaluate=async expression=>{const reply=await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true,userGesture:true});if(reply.exceptionDetails)throw new Error(JSON.stringify(reply.exceptionDetails));return reply.result.value;};
 try{
  await until(()=>evaluate('Boolean(window.FitsViewer?.state())'),90);
  screenshot('01-startup.png');
  await evaluate('document.querySelector("#file").click()');
  const systemRecovery=[];
  let xml=await until(()=>{
   const text=dump();
   if(/isn't responding|is not responding/.test(text)){
    const elements=nodes(text),title=elements.find(node=>node.package==='android'&&node['resource-id']==='android:id/alertTitle'&&node.text==="Pixel Launcher isn't responding");
    const close=elements.find(node=>node.package==='android'&&node['resource-id']==='android:id/aerr_close'&&node.text==='Close app');
    if(title&&close&&systemRecovery.length<2&&tapBounds(close)){
     systemRecovery.push({dialogPackage:title.package,title:title.text,actionId:close['resource-id'],action:close.text});
     fs.writeFileSync(out+'/system-recovery.json',JSON.stringify(systemRecovery,null,2));
     return null;
    }
    const error=new Error('Unexpected or repeated Android ANR; preserve failure evidence');error.fatal=true;throw error;
   }
   return text.includes('com.google.android.documentsui')||text.includes('com.android.documentsui')?text:null;
  });
  screenshot('02-system-picker.png');fs.writeFileSync(out+'/picker.xml',xml);
  const documentNodes=text=>nodes(text).filter(node=>/^(com\.google\.android|com\.android)\.documentsui$/.test(node.package||''));
  const fileNode=text=>documentNodes(text).find(node=>node.text==='mobile-fixture.fits');
  if(!fileNode(xml)){
   const drawer=documentNodes(xml).find(node=>/^(Show roots|Open navigation drawer)$/.test(node['content-desc']||''));
   if(!drawer||!tapBounds(drawer))throw new Error('Native DocumentsUI navigation drawer was not found');
   xml=await until(()=>{const current=dump();return documentNodes(current).some(node=>/^Downloads?$/.test(node.text||''))?current:null;},30);
   fs.writeFileSync(out+'/roots.xml',xml);screenshot('02a-roots.png');
   const downloads=documentNodes(xml).find(node=>/^Downloads?$/.test(node.text||''));
   if(!tapBounds(downloads))throw new Error('Could not select Downloads in the native file picker');
   xml=await until(()=>{const current=dump();return fileNode(current)?current:null;},30);
  }
  fs.writeFileSync(out+'/downloads.xml',xml);screenshot('02b-downloads.png');
  const selected=fileNode(xml);if(!selected||!tapBounds(selected))throw new Error('Native picker did not expose the exact local FITS filename label');
  fs.writeFileSync(out+'/selection-target.json',JSON.stringify(selected,null,2));
  xml=await until(()=>{
   const current=dump(),elements=nodes(current);
   fs.writeFileSync(out+'/after-selection.xml',current);
   if(elements.some(node=>node.package==='com.google.android.apps.photos')){const error=new Error('Native selection opened Google Photos rather than returning to FitsViewer');error.fatal=true;throw error;}
   return elements.some(node=>node.package==='org.fitsviewer.mini.test')&&!documentNodes(current).length?current:null;
  },30);
  screenshot('02c-picker-returned.png');
  // The shared loader clears input.value after consuming its bytes.
  const chosen=await evaluate('document.querySelector("#status").textContent');assert.equal(chosen,'mobile-fixture.fits');
  const loaded=await until(()=>evaluate('window.FitsViewer.state().loaded&&window.FitsViewer.state().texture?window.FitsViewer.state():null'),90);
  fs.writeFileSync(out+'/loaded-state.json',JSON.stringify(loaded,null,2));
  assert.equal(loaded.width,64);assert.equal(loaded.build_features.pro,false);
  const before=loaded.zoom;
  const b=await evaluate('document.querySelector("#zoom-in").getBoundingClientRect().toJSON()');
  await call('Input.dispatchTouchEvent',{type:'touchStart',touchPoints:[{x:b.x+b.width/2,y:b.y+b.height/2,id:1}]});
  await call('Input.dispatchTouchEvent',{type:'touchEnd',touchPoints:[]});
  await until(()=>evaluate(`window.FitsViewer.state().zoom>${before}`));screenshot('03-local-fits.png');
  await evaluate('document.querySelector("#close").click()');await until(()=>evaluate('!window.FitsViewer.state().loaded'));
  fs.writeFileSync(out+'/result.json',JSON.stringify({nativeSystemPicker:true,localFile:'mobile-fixture.fits',proExcluded:true,texture:true,touchZoomButton:true,close:true,systemRecoveryCount:systemRecovery.length,systemRecovery,device:adb('shell','getprop','ro.product.model')},null,2));
 }finally{socket.close();adb('shell','am','force-stop','org.fitsviewer.mini.test');adb('forward','--remove','tcp:19227');}
})().catch(error=>{try{screenshot('failure.png');fs.writeFileSync(out+'/failure-ui.xml',dump());}catch{}console.error(error);process.exitCode=1;});
