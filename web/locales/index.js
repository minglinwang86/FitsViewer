/** Shared shell translator. Dictionary keys, scientific identifiers and paths never change. */
export function createLocalization(initialLanguage='en',options={}) {
 const loadJson=options.loadJson??(async file=>{
  const response=await fetch(new URL(file,import.meta.url),{credentials:'same-origin',referrerPolicy:'no-referrer'});
  if(!response.ok)throw new Error(`Locale resource unavailable (${response.status})`);
  return response.json();
 });
 let registry={default:'en',languages:[{code:'en',name:'English',label:'Language: English',file:'en.json'}]};
 const registryReady=loadJson('registry.json').then(value=>{registry=value;}).catch(()=>{});
 const cache=new Map();let language=registry.default,dictionary=null,revision=0;
 const translator={
  get language(){return language;},
  get languages(){return registry.languages;},
  text(key,values={}){
   const translated=dictionary&&Object.hasOwn(dictionary,key)?dictionary[key]:undefined;
   const text=typeof translated==='string'?translated:key;
   return text.replace(/\{(\w+)\}/g,(match,name)=>Object.hasOwn(values,name)?String(values[name]):match);
  },
  async setLanguage(requested){
   await registryReady;
   const selected=registry.languages.find(locale=>locale.code===requested)??registry.languages.find(locale=>locale.code===registry.default);
   const current=++revision;
   // English keys are the fallback: do not parse 910 messages just to show English.
   if(selected.code===registry.default){language=selected.code;dictionary=null;return language;}
   if(!cache.has(selected.code))cache.set(selected.code,loadJson(selected.file).catch(()=>null));
   const messages=await cache.get(selected.code);
   if(current===revision){language=selected.code;dictionary=messages;}
   return language;
  }
 };
 translator.ready=translator.setLanguage(initialLanguage);
 return translator;
}
