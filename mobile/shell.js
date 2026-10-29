// Mobile controls call the shared view API; scientific coordinates stay in Rust.
(() => {
 // Normalize before eframe's observer callback paints a frame. Repairing only
 // afterwards can briefly expose a nonzero half-size viewport during tab restore.
 const NativeResizeObserver=window.ResizeObserver;
 window.ResizeObserver=class extends NativeResizeObserver{
  constructor(callback){super((entries,observer)=>callback(entries.map(entry=>{
   if(entry.target.id!=='canvas'||!entry.devicePixelContentBoxSize?.length)return entry;
   const dimensions=[Math.round(entry.contentRect.width*(devicePixelRatio||1)),Math.round(entry.contentRect.height*(devicePixelRatio||1))];
   const boxes=Array.from(entry.devicePixelContentBoxSize,size=>new Proxy(size,{get(target,key){if(key==='inlineSize')return dimensions[0];if(key==='blockSize')return dimensions[1];return Reflect.get(target,key,target);}}));
   return new Proxy(entry,{get(target,key){return key==='devicePixelContentBoxSize'?boxes:Reflect.get(target,key,target);}});
  }),observer));}
 };
 const api=()=>window.FitsViewer;
 const report=error=>{document.getElementById('status').textContent=String(error);};
 const destination=document.getElementById('destination');
 const sharedLabels=JSON.parse(document.getElementById('mobile-shared-labels').textContent);
 destination.onpointerdown=()=>{destination.value=api()?.state()?.preferences?.minimal_open_destination||'replace';};
 destination.onchange=()=>{const preferences=api()?.state()?.preferences;if(preferences)api().command('preferences',{...preferences,minimal_open_destination:destination.value}).catch(report);};
 for(const [id,factor] of [['zoom-in',1.25],['zoom-out',.8]]){
  document.getElementById(id).onclick=async()=>{const state=api()?.state();if(!state?.loaded)return;try{
   if(state.spectrum){const bounds=state.plot_bounds.slice();for(const i of [0,2]){const center=(bounds[i]+bounds[i+1])/2,half=(bounds[i+1]-bounds[i])/(2*factor);bounds[i]=center-half;bounds[i+1]=center+half;}await api().command('view',{plot_bounds:bounds});}
   else await api().command('view',{zoom:Math.max(.005,Math.min(128,state.zoom*factor)),pan:state.pan.map(v=>v*factor)});
  }catch(error){report(error);}};
 }
 document.getElementById('fit').onclick=()=>api()?.command('action',{name:'Fit'}).catch(report);
 // A complete inline HTML also opens inside WKWebView without local fetch/CORS exceptions.
 // Only the HTTPS web deployment registers an offline cache. Native shells bundle it.
 if(window.isSecureContext&&location.hostname!=='appassets.androidplatform.net'&&'serviceWorker' in navigator)navigator.serviceWorker.register('./sw.js').catch(report);
 // eframe 0.32 trusts ResizeObserver.devicePixelContentBoxSize. Some embedded
 // and emulated browsers report CSS pixels there despite a non-unit DPR. Repair
 // only the backing buffer, retaining CSS size and Rust's coordinate transform.
 const canvas=document.getElementById('canvas');let resizeObserver,frame;
 function repairBuffer(){
  cancelAnimationFrame(frame);frame=requestAnimationFrame(()=>{
   const rect=canvas.getBoundingClientRect(),dpr=devicePixelRatio||1;
   const width=Math.round(rect.width*dpr),height=Math.round(rect.height*dpr);
   if(width&&height&&(canvas.width!==width||canvas.height!==height)){
    canvas.width=width;canvas.height=height;api().command('state').catch(report);
   }
  });
 }
 const waitForRunner=setInterval(()=>{
  if(!api()?.state())return;clearInterval(waitForRunner);
  const originalLanguage=api().setLanguage;
  api().setLanguage=language=>{originalLanguage?.(language);const labels=sharedLabels[language]||sharedLabels.en;for(const option of destination.options)option.textContent=labels[option.value==='tab'?'Open in new tab':'Replace current'];document.getElementById('fit').textContent=labels.Fit;};
  api().setLanguage(api().state().preferences.language);destination.value=api().state().preferences.minimal_open_destination;
  resizeObserver=new ResizeObserver(repairBuffer);resizeObserver.observe(canvas);
  window.addEventListener('resize',repairBuffer);repairBuffer();
 },30);
 window.addEventListener('pagehide',()=>{clearInterval(waitForRunner);resizeObserver?.disconnect();cancelAnimationFrame(frame);window.removeEventListener('resize',repairBuffer);},{once:true});
 window.addEventListener('pageshow',event=>{if(event.persisted)location.reload();});
})();
