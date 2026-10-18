"""Create a separate, offline single-file HTML asset; does not change wheel resources.
生成独立离线 HTML；不将另一份 WASM 或字体写入基础 wheel。
"""
from pathlib import Path
import argparse,base64,html as html_escape
from urllib.parse import quote
parser=argparse.ArgumentParser();parser.add_argument('--pkg',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
javascript=(args.pkg/'fitsviewer_web.js').read_text(encoding='utf-8')
if any(line.lstrip().startswith('import ') for line in javascript.splitlines()):raise RuntimeError('External JS imports must be inlined before producing a standalone file')
wasm=base64.b64encode((args.pkg/'fitsviewer_web_bg.wasm').read_bytes()).decode()
font=base64.b64encode((root/'web/fonts/cjk.ttf').read_bytes()).decode()
html='''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>FitsViewer offline</title><style>html,body{margin:0;background:#0d1117;color:#ddd;height:100%;font:13px system-ui}body{display:flex;flex-direction:column}header{display:flex;align-items:center;gap:12px;padding:6px 12px;flex-wrap:wrap}#stage{position:relative;flex:1;min-height:0}canvas{width:100%;height:100%;display:block}button,input{font:inherit}#status{color:#aaa}</style><header><strong id="app-title">FitsViewer offline</strong><input id="file" type="file" accept=".fits,.fit,.fts,.fz,.gz,.txt,.dat,.csv,.ascii"><button id="close">Close image</button><span id="status">Starting…</span><small>Local images, compressed FITS and basic spectra. 64 MiB input/decoded limit. No server or scientific Python tools.</small></header><div id="stage"><canvas id="canvas"></canvas></div><script type="module">'''
favicon='data:image/svg+xml,'+quote((root/'web/icons/fitsviewer.svg').read_text(encoding='utf-8'),safe='')
html=html.replace('<title>', '<link rel="icon" type="image/svg+xml" href="'+favicon+'"><title>',1)
notices=[];seen=set()
for path in [root/'LICENSE',root/'NOTICE.md',root/'THIRD_PARTY_NOTICES.md',*sorted((root/'licenses').rglob('*'))]:
    if path.is_file() and path.suffix not in ['.json','.png','.ttf']:
        text=path.read_text(encoding='utf-8',errors='replace')
        if text not in seen:notices.append(str(path.relative_to(root))+'\n'+text);seen.add(text)
html=html.replace('<span id="status">','<button id="license-button" onclick="document.getElementById(\'licenses\').hidden=!document.getElementById(\'licenses\').hidden">Licenses</button><span id="status">')
html=html.replace('<div id="stage">','<div id="licenses" hidden style="position:absolute;inset:50px 15px;overflow:auto;background:#161b22;z-index:9"><pre style="white-space:pre-wrap">'+html_escape.escape('\n\n'.join(notices))+'</pre></div><div id="stage">')
html+=javascript
html+='''\nconst bytes=text=>Uint8Array.from(atob(text),c=>c.charCodeAt(0));
const status=document.getElementById('status');let ready=false,queue=Promise.resolve();
window.FitsViewer={state:()=>window.__fitsviewerState,command(operation,params={}){
 const task=()=>new Promise((resolve,reject)=>{const id=crypto.randomUUID();command(JSON.stringify({id,operation,params}));const limit=Date.now()+15000;function poll(){const r=window.__fitsviewerCommandResult;if(r?.id===id){r.ok?resolve(r.result):reject(new Error(r.error));return;}if(Date.now()>limit){reject(new Error('Command timed out'));return;}setTimeout(poll,20);}poll();});const result=queue.then(task);queue=result.catch(()=>{});return result;
},download(name,text){const url=URL.createObjectURL(new Blob([text],{type:'application/json;charset=utf-8'})),link=document.createElement('a');link.href=url;link.download=name.split(/[\\\\/]/).at(-1)||'fitsviewer.json';link.click();setTimeout(()=>URL.revokeObjectURL(url),1000);},
importPreferences(){const input=document.createElement('input');input.type='file';input.accept='.json';input.onchange=async()=>{const file=input.files?.[0];if(!file)return;if(file.size>65536){setStatus('preferences');return;}try{await window.FitsViewer.command('preferences',JSON.parse(await file.text()));}catch(error){setStatus('error',String(error));}};input.click();}};
let language='en',statusKind='starting',statusArgument='';
const labels={starting:['Starting…','正在启动…'],choose:['Choose or drop a local file','选择或拖入本地文件'],loading:['Loading ','正在载入 '],large:['File exceeds 64 MiB','文件超过 64 MiB'],preferences:['Preferences exceed 64 KiB','偏好设置超过 64 KiB']};
function setStatus(kind,argument=''){statusKind=kind;statusArgument=argument;status.textContent=labels[kind]?labels[kind][language==='zh-CN'?1:0]+argument:argument;}
window.FitsViewer.setLanguage=value=>{language=value;const chinese=value==='zh-CN';document.documentElement.lang=chinese?'zh-CN':'en';document.title=chinese?'FitsViewer 离线查看器':'FitsViewer offline';document.getElementById('app-title').textContent=document.title;document.getElementById('close').textContent=chinese?'关闭图像':'Close image';document.getElementById('license-button').textContent=chinese?'许可证':'Licenses';document.getElementById('file').setAttribute('aria-label',chinese?'选择本地 FITS 或光谱文件':'Choose a local FITS image or spectrum');document.querySelector('header small').textContent=chinese?'本地图像、压缩 FITS 和基础光谱。输入及解压上限 64 MiB。无服务器及科学 Python 工具。':'Local images, compressed FITS and basic spectra. 64 MiB input/decoded limit. No server or scientific Python tools.';setStatus(statusKind,statusArgument);};
window.FitsViewer.importRegions=()=>{const input=document.createElement('input');input.type='file';input.accept='.json';input.onchange=async()=>{try{const file=input.files?.[0];if(!file)return;if(file.size>262144)throw new Error('Region template exceeds 256 KiB');await window.FitsViewer.command('aperture.template_load',{template:JSON.parse(await file.text())});}catch(error){setStatus('error',String(error));}};input.click();};
async function load(file){if(!ready)return;if(file.size>64*1024*1024){setStatus('large');return;}try{setStatus('loading',file.name);open_file(file.name,new Uint8Array(await file.arrayBuffer()));setStatus('file',file.name);}catch(error){setStatus('error',String(error));}document.getElementById('file').value='';}
document.getElementById('file').onchange=event=>{const file=event.target.files?.[0];if(file)load(file);};
document.body.ondragover=event=>event.preventDefault();document.body.ondrop=event=>{event.preventDefault();const file=event.dataTransfer?.files?.[0];if(file)load(file);};
document.getElementById('canvas').oncontextmenu=event=>event.preventDefault();
document.getElementById('close').onclick=()=>window.FitsViewer.command('close');
window.addEventListener('pagehide',()=>stop(),{once:true});
try{await __wbg_init({module_or_path:bytes('''+repr(wasm)+''')});await start(document.getElementById('canvas'),'','','',undefined,bytes('''+repr(font)+'''));ready=true;setStatus('choose');}catch(error){setStatus('error',String(error));}
</script></html>'''
args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(html,encoding='utf-8');print(args.output.name,args.output.stat().st_size,'bytes; separate offline asset, no installation packages included')

