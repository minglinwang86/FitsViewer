"""Headless, reproducible commands; scientific libraries come from the chosen environment.
无桌面脚本接口，测光复用 Rust 内核，科研绘图调用用户环境。
"""
import argparse
import json
import math
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from .runtime import resources,process_environment

def read_json(path):
    text=sys.stdin.read(1024*1024+1) if path=='-' else Path(path).read_text(encoding='utf-8-sig')
    if len(text)>1024*1024:raise ValueError('JSON input exceeds 1 MiB')
    return json.loads(text)

def binary_json(flag,path=None,value=None):
    _,binary,_=resources();command=[str(binary),flag]
    if path is not None:command.append(str(path))
    options={'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {}
    result=subprocess.run(command,input=None if value is None else json.dumps(value,ensure_ascii=False).encode('utf-8'),capture_output=True,env=dict(os.environ,**process_environment()),timeout=120,**options)
    if result.returncode:raise RuntimeError(result.stderr.decode('utf-8','replace').strip() or 'FITS core operation failed')
    return json.loads(result.stdout)

def worker_call(operation,params,output=None,python=None):
    root,binary,web=resources()
    script=web/'python/fitsviewer_science.py'
    interpreter=python or os.environ.get('FITSVIEWER_PYTHON') or sys.executable
    with tempfile.TemporaryDirectory(prefix='fitsviewer-science-') as directory:
        environment=dict(os.environ,**process_environment(),FITSVIEWER_CORE_BINARY=str(binary),PYTHONIOENCODING='utf-8',MPLBACKEND='Agg')
        options={'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {}
        with subprocess.Popen([interpreter,'-u',str(script),'--cache',directory],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=environment,**options) as process:
            request=json.dumps({'id':1,'operation':operation,'params':params},ensure_ascii=False).encode('utf-8')+b'\n'+b'{"operation":"shutdown"}\n'
            try:stdout,stderr=process.communicate(request,timeout=180)
            except subprocess.TimeoutExpired:
                process.kill();process.communicate();raise RuntimeError('Scientific operation timed out')
            if process.returncode:raise RuntimeError(stderr.decode('utf-8','replace')[-8000:] or 'Scientific process stopped')
            line=next((line for line in stdout.splitlines() if line.strip()),b'{}');response=json.loads(line)
            if not response.get('ok'):raise RuntimeError(response.get('error','Invalid scientific response'))
            result=response['result']
            if output:
                if not result.get('_path'):raise ValueError('Operation did not create an export artifact')
                target=Path(output).expanduser()
                if os.name=='nt':
                    text=os.path.abspath(target)
                    if not text.startswith('\\\\?\\'):text='\\\\?\\UNC\\'+text[2:] if text.startswith('\\\\') else '\\\\?\\'+text
                    target=Path(text)
                target=target.resolve();target.parent.mkdir(parents=True,exist_ok=True)
                source=Path(result['_path']);shutil.copy2(source,target);result['output']=str(target)
                if isinstance(result.get('provenance'),dict) and result['provenance'].get('_path'):
                    sidecar=target.with_suffix(target.suffix+'.json');shutil.copy2(result['provenance']['_path'],sidecar);result['provenance']['output']=str(sidecar)
            def strip(value):
                if isinstance(value,dict):return {k:strip(v) for k,v in value.items() if k not in ['_path','artifact_id']}
                if isinstance(value,list):return [strip(v) for v in value]
                return value
            return strip(result)

def validate_spectrum_overlay(value):
    """Validate presentation metadata without importing a scientific package.

    仅校验对照谱显示参数，不读取或重采样光谱。
    """
    if not isinstance(value,dict):raise ValueError('Spectrum overlay must be an object')
    if set(value)-{'enabled','hidden','opacity','order','label','epoch','scale','offset','unit','redshift','air'}:raise ValueError('Unknown spectrum overlay setting')
    for key in ('enabled','hidden','air'):
        if key in value and not isinstance(value[key],bool):raise ValueError(key+' must be boolean')
    for key in ('label','epoch'):
        if key in value and (not isinstance(value[key],str) or len(value[key])>120):raise ValueError(key+' must be at most 120 characters')
    if 'unit' in value and value['unit'] not in ('','Angstrom','nm','um'):raise ValueError('Choose Angstrom, nm or um')
    if 'opacity' in value and (isinstance(value['opacity'],bool) or not isinstance(value['opacity'],(int,float)) or not math.isfinite(value['opacity']) or not 0<=value['opacity']<=1):raise ValueError('Opacity must be between 0 and 1')
    if 'order' in value and (isinstance(value['order'],bool) or not isinstance(value['order'],int) or not -10000<=value['order']<=10000):raise ValueError('Invalid layer order')
    for key in ('scale','offset','redshift'):
        if key not in value:continue
        number=value[key]
        if isinstance(number,bool) or not isinstance(number,(float,int)) or not math.isfinite(number):raise ValueError(key+' must be finite')
        if key=='scale' and (number==0 or abs(number)>1e12):raise ValueError('Scale must be nonzero and at most 1e12')
        if key=='offset' and abs(number)>1e30:raise ValueError('Offset must be at most 1e30')
        if key=='redshift' and not -1<number<=20:raise ValueError('Redshift must be -1 < z <= 20')
    return dict(value)


def validate_session(value,base=None):
    if not isinstance(value,dict) or value.get('schema',1)!=1:raise ValueError('Session schema must be 1')
    views=value.get('views',[])
    if not 1<=len(views)<=32:raise ValueError('Session needs one to 32 views')
    result=[]
    for view in views:
        path=view.get('path')
        if not isinstance(path,str):raise ValueError('Every session view requires a file path')
        path=str((Path(base)/path if base and not Path(path).is_absolute() else Path(path)).expanduser().resolve())
        metadata=binary_json('--inspect-json',path)
        config=binary_json('--validate-view',value=view.get('view',{}))
        apertures=view.get('apertures',[])
        if not isinstance(apertures,list) or len(apertures)>64:raise ValueError('Session supports at most 64 apertures per view')
        entry=dict(path=path,hdu=view.get('hdu',metadata['dataset']['hdu']),view=config,apertures=apertures)
        if 'spectrum_overlay' in view:entry['spectrum_overlay']=validate_spectrum_overlay(view['spectrum_overlay'])
        if 'comparison_view' in view:
            comparison=view['comparison_view']
            if not isinstance(comparison,dict) or set(comparison)-{'zoom','pan','fit'}:raise ValueError('Invalid comparison view')
            zoom=comparison.get('zoom',1.);pan=comparison.get('pan',[0.,0.]);fit=comparison.get('fit',True)
            if isinstance(zoom,bool) or not isinstance(zoom,(int,float)) or not math.isfinite(zoom) or not .005<=zoom<=128:raise ValueError('Invalid comparison zoom')
            if not isinstance(pan,list) or len(pan)!=2 or any(isinstance(v,bool) or not isinstance(v,(int,float)) or not math.isfinite(v) or abs(v)>1e8 for v in pan) or not isinstance(fit,bool):raise ValueError('Invalid comparison pan/fit')
            entry['comparison_view']=dict(zoom=zoom,pan=pan,fit=fit)
        result.append(entry)
    if sum(bool(view.get('spectrum_overlay',{}).get('enabled')) for view in result)>8:raise ValueError('At most 8 enabled comparison spectra')
    mode=value.get('mode','PRO' if len(result)>1 else 'Minimal')
    if mode not in ['Minimal','PRO']:raise ValueError('Unknown session mode')
    restored={'schema':1,'views':result,'mode':mode,'validated':True}
    for key in ('selected','secondary'):
        if value.get(key) is not None:
            index=value[key]
            if isinstance(index,bool) or not isinstance(index,int) or not 0<=index<len(result):raise ValueError('Invalid session '+key)
            restored[key]=index
    if 'link' in value:
        if value['link'] not in ('Independent','Pixel','Sky'):raise ValueError('Invalid session link')
        restored['link']=value['link']
    return restored


def apply_session(value,args):
    """Restore a validated session through the real native/browser entry point."""
    from .runtime import runtime_dir
    _,binary,web=resources()
    target=runtime_dir()/('session-'+__import__('uuid').uuid4().hex+'.json')
    target.write_text(json.dumps(value,ensure_ascii=False),encoding='utf-8')
    command=[str(binary),'--session',str(target),'--web-dir',str(web)]
    if args.python:command.extend(['--python',args.python])
    if args.native:
        command.append('--native')
        options={'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {}
        process=subprocess.Popen(command,stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=dict(os.environ,**process_environment()),**options)
        return dict(value,application='native',state='launched',pid=process.pid,session_file=str(target))
    from .cli import _warm_jupyter
    _warm_jupyter()
    root=args.root or os.path.commonpath([str(Path(view['path']).parent) for view in value['views']])
    command.extend(['--browser','--root',root,'--port',str(args.port)])
    if args.no_browser:command.append('--no-browser')
    try:
        response=subprocess.run(command,capture_output=True,timeout=45,env=dict(os.environ,**process_environment()))
        if response.returncode:raise RuntimeError(response.stderr.decode('utf-8','replace').strip() or 'Session launch failed')
        url=next((line for line in response.stdout.decode('utf-8').splitlines() if line.startswith(('http://','https://'))),None)
        if not url:raise RuntimeError('Viewer did not return a session URL')
        return dict(value,application='browser',state='queued',url=url)
    finally:target.unlink(missing_ok=True)

def command(arguments):
    name=arguments[0];parser=argparse.ArgumentParser(prog='fitsviewer '+name)
    parser.add_argument('--json',action='store_true',help='Print stable JSON (also the default)')
    if name=='inspect':
        parser.add_argument('path');parser.add_argument('--hdu',type=int)
    elif name=='features':parser.add_argument('--python')
    elif name=='render':
        parser.add_argument('path');parser.add_argument('--view');parser.add_argument('--params');parser.add_argument('--output',required=True);parser.add_argument('--width',type=int,default=1600);parser.add_argument('--height',type=int,default=1200);parser.add_argument('--dpi',type=float,default=150);parser.add_argument('--python')
    elif name=='measure':
        parser.add_argument('path');parser.add_argument('--regions',required=True);parser.add_argument('--error-path');parser.add_argument('--hdu',type=int)
    elif name=='science':
        parser.add_argument('operation');parser.add_argument('--params',required=True);parser.add_argument('--output');parser.add_argument('--python')
    elif name=='batch':parser.add_argument('--input',default='-')
    elif name=='session':
        parser.add_argument('action',choices=['apply','validate']);parser.add_argument('file')
        parser.add_argument('--native',action='store_true');parser.add_argument('--no-browser',action='store_true');parser.add_argument('--python');parser.add_argument('--root');parser.add_argument('--port',type=int,default=0)
    else:raise ValueError('Unknown headless command')
    args=parser.parse_args(arguments[1:])
    if name=='inspect':
        if args.hdu is None:result=binary_json('--inspect-json',args.path)
        else:
            _,binary,_=resources();r=subprocess.run([str(binary),'--inspect-json',args.path,'--hdu',str(args.hdu)],capture_output=True,env=dict(os.environ,**process_environment()),timeout=120)
            if r.returncode:raise RuntimeError(r.stderr.decode('utf-8','replace'))
            result=json.loads(r.stdout)
    elif name=='features':result=worker_call('capabilities',{},python=args.python)
    elif name=='measure':
        params=read_json(args.regions)
        if isinstance(params,list):params={'apertures':params}
        params['path']=args.path
        if args.hdu is not None:params['hdu']=args.hdu
        if args.error_path:params['error_path']=args.error_path
        result=binary_json('--measure-json',value=params)
    elif name=='render':
        view=read_json(args.view) if args.view else {}
        if view:view=binary_json('--validate-view',value=view)
        params=read_json(args.params) if args.params else {}
        params.update(path=args.path,view=view,width=args.width,height=args.height,dpi=args.dpi,format=Path(args.output).suffix.lstrip('.') or 'png')
        result=worker_call('export',params,args.output,args.python)
    elif name=='science':
        operation=args.operation
        allowed={'wcs','catalogue','xmatch','detect_stars','detect_lines','trace_curve','line_profile','spectral_candidates','spectral_analysis','image_flux_match','region_transform','reproject','rgb','export','math_preview','vo_query','vo_download','xmatch_cds','gaia_solve','capabilities'}
        if operation not in allowed:raise ValueError('Unsupported stateless operation; use samp-bridge for a persistent SAMP client')
        result=worker_call(operation,read_json(args.params),args.output,args.python)
    elif name=='session':
        result=validate_session(read_json(args.file),Path(args.file).resolve().parent if args.file!='-' else Path.cwd())
        if args.action=='apply':result=apply_session(result,args)
    else:
        stream=sys.stdin if args.input=='-' else open(args.input,encoding='utf-8-sig')
        failed=False
        try:
            for index,line in enumerate(stream):
                if index>=1000:raise ValueError('Batch limit is 1000 operations')
                if not line.strip():continue
                identifier=None
                try:
                    if len(line)>1024*1024:raise ValueError('Batch request exceeds 1 MiB')
                    request=json.loads(line);identifier=request.get('id');operation=request['operation'];params=request.get('params',{})
                    if operation=='inspect':item=binary_json('--inspect-json',params['path'])
                    elif operation=='measure':item=binary_json('--measure-json',value=params)
                    elif operation=='view.validate':item=binary_json('--validate-view',value=params)
                    else:raise ValueError('Batch operation must be inspect, measure or view.validate')
                    response={'id':identifier,'ok':True,'result':item}
                except (ValueError,KeyError,OSError,RuntimeError,subprocess.SubprocessError) as error:failed=True;response={'id':identifier,'ok':False,'error':str(error)}
                print(json.dumps(response,ensure_ascii=False,allow_nan=False),flush=True)
        finally:
            if stream is not sys.stdin:stream.close()
        return 1 if failed else 0
    print(json.dumps(result,ensure_ascii=False,allow_nan=False,indent=2))
    return 0

