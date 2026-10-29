// Exercise the actual shared browser translator, without a browser or scientific worker.
import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
const root=new URL('../web/locales/',import.meta.url);
const code=await fs.readFile(new URL('index.js',root),'utf8');
const {createLocalization}=await import('data:text/javascript;base64,'+Buffer.from(code).toString('base64'));
const read=async file=>JSON.parse(await fs.readFile(new URL(file,root),'utf8'));
const calls=[];
const locale=createLocalization('en',{loadJson:async file=>{calls.push(file);return read(file);}});
await locale.ready;
assert.deepEqual(calls,['registry.json'],'English startup must not parse all dictionaries');
const scientific=['FLUX_ERR','RA---TAN','Hα','1.234e-16','C:\\dati\\étoile.fits','中文路径.fits','toString','constructor','__proto__'];
for(const entry of (await read('registry.json')).languages){
 await locale.setLanguage(entry.code);
 const dictionary=await read(entry.file);
 assert.equal(locale.language,entry.code);
 assert.equal(locale.text('Open'),dictionary.Open);
 for(const key of scientific)assert.equal(locale.text(key),key,entry.code+': '+key);
 const template='Target {target}: unable to display file: {error}';
 assert.equal(locale.text(template,{target:'M67/é.fits',error:'FLUX_ERR'}),dictionary[template].replace('{target}','M67/é.fits').replace('{error}','FLUX_ERR'));
}
await locale.setLanguage('unsupported');assert.equal(locale.language,'en');assert.equal(locale.text('Open'),'Open');
let release;const delayed=createLocalization('en',{loadJson:file=>file==='fr.json'?new Promise(resolve=>{release=()=>read(file).then(resolve);}):read(file)});
await delayed.ready;
const stale=delayed.setLanguage('fr');await delayed.setLanguage('de');release();await stale;
assert.equal(delayed.language,'de');assert.equal(delayed.text('Open'),(await read('de.json')).Open);
const missing=createLocalization('es',{loadJson:file=>file==='es.json'?Promise.reject(new Error('controlled missing resource')):read(file)});
await missing.ready;assert.equal(missing.text('Open'),'Open');assert.equal(missing.text('constructor'),'constructor');
const unavailable=createLocalization('fr',{loadJson:()=>Promise.reject(new Error('controlled unavailable registry'))});
await unavailable.ready;assert.equal(unavailable.language,'en');assert.equal(unavailable.text('Open'),'Open');
console.log(JSON.stringify({passed:true,languages:6,checks:['same dictionaries','unknown/prototype scientific fields','placeholders','English lazy startup','rapid-switch stale response','missing dictionary fallback','missing registry fallback']}));
