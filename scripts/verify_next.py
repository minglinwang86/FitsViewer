"""Validate a candidate wheel in an isolated environment; never touches production Jupyter.
在独立目录验证候选包，不接触正在使用的 Jupyter 服务。
"""
from pathlib import Path
import argparse
import hashlib
import json
import math
import os
import secrets
import shutil
import socket
import struct
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--wheel',type=Path,required=True)
parser.add_argument('--scratch',type=Path,required=True)
parser.add_argument('--port',type=int,default=0)
parser.add_argument('--keep',action='store_true')
parser.add_argument('--native',action='store_true')
parser.add_argument('--capture-python',type=Path)
args=parser.parse_args()
root=Path(__file__).resolve().parents[1]
scratch=args.scratch.resolve()
assert not scratch.exists(),'Use a new scratch directory'
scratch.mkdir(parents=True)
data=scratch/'data'
shutil.copytree(root/'examples/next',data)
shutil.copy2(root/'examples/starfield.fits',data/'science.fits')
# A small NaN patch remains targetable after CSS/device-pixel rounding.
# 小片无效区域便于在不同 DPI 下稳定验证游标文本变化与布局。
fixture=bytearray((data/'science.fits').read_bytes())
end=next(i+80 for i in range(0,len(fixture),80) if fixture[i:i+8]==b'END     ')
offset=(end+2879)//2880*2880
for y in range(16,25):
    for x in range(16,25):struct.pack_into('>f',fixture,offset+4*(y*512+x),float('nan'))
(data/'science.fits').write_bytes(fixture)
env={k:v for k,v in os.environ.items() if not k.startswith(('FITSVIEWER_','JUPYTER_','PYTHONPATH'))}
env.update(FITSVIEWER_RUNTIME_DIR=str(scratch/'viewer-runtime'),FITSVIEWER_CONFIG=str(scratch/'settings.json'),
           JUPYTER_CONFIG_DIR=str(scratch/'config'),JUPYTER_DATA_DIR=str(scratch/'jupyter-data'),JUPYTER_RUNTIME_DIR=str(scratch/'runtime'),
           JUPYTER_PREFER_ENV_PATH='1',PYTHONIOENCODING='utf-8',PYTHONNOUSERSITE='1',XDG_CACHE_HOME=str(scratch/'cache'),XDG_CONFIG_HOME=str(scratch/'preferences'))
log=(scratch/'test.log').open('w',encoding='utf-8')
def run(command,**kwargs):
    return subprocess.run([str(p) for p in command],env=env,check=True,stdout=log,stderr=subprocess.STDOUT,**kwargs)
def stop(p):
    if p and p.poll() is None:
        import psutil
        for child in reversed(psutil.Process(p.pid).children(recursive=True)):
            try:child.terminate()
            except psutil.NoSuchProcess:pass
        p.terminate()
        try:p.wait(timeout=10)
        except subprocess.TimeoutExpired:p.kill();p.wait()
def wait_json(path,process):
    for _ in range(240):
        if process.poll() is not None:raise RuntimeError('Test server exited; inspect test.log')
        try:return json.loads(path.read_text(encoding='utf-8'))
        except (FileNotFoundError,json.JSONDecodeError,PermissionError):time.sleep(.25)
    raise TimeoutError(str(path))
def get(url,token=None):
    return urllib.request.urlopen(urllib.request.Request(url,headers={'X-Fitsviewer-Token':token} if token else {}),timeout=45)
run([sys.executable,'-m','pip','install','--no-deps','--force-reinstall',args.wheel.resolve()])
run([sys.executable,'-m','pip','check'])
from fitsviewer.runtime import resources
import importlib.metadata
vendor,binary,web=resources()
report={'version':importlib.metadata.version('fitsviewer'),'jupyterlab':importlib.metadata.version('jupyterlab'),
        'python':sys.version.split()[0],'platform':sys.platform,'wheel':args.wheel.name,'wheel_bytes':args.wheel.stat().st_size,
        'sha256':hashlib.sha256(args.wheel.read_bytes()).hexdigest(),'cases':[]}
with zipfile.ZipFile(args.wheel) as archive:
    font=archive.read('fitsviewer/_vendor/web/fonts/cjk.ttf')
    font_manifest=json.loads(archive.read('fitsviewer/_vendor/web/fonts/font-manifest.json'))
    assert len(font)==font_manifest['bytes'] and hashlib.sha256(font).hexdigest()==font_manifest['asset_sha256']
    assert any(n.endswith('/droid/LICENSE-APACHE.txt') for n in archive.namelist())
    report['shared_cjk_font_bytes']=len(font)
server=jupyter=display_process=None
success=False
try:
    server=subprocess.Popen([str(binary),'--serve','--port','0','--root',str(data)],env=env,stdout=log,stderr=subprocess.STDOUT)
    info=wait_json(scratch/'viewer-runtime/server.json',server)
    base=f"http://127.0.0.1:{info['port']}"
    for case in json.loads((data/'reference.json').read_text(encoding='utf-8')):
        query=urllib.parse.urlencode({'path':case['file']})
        try:
            with get(base+'/api/image?'+query,info['token']) as response:packet=response.read()
        except urllib.error.HTTPError as error:
            assert not case['expected_supported'] and error.code==422,(case['file'],error.code,error.read().decode())
            report['cases'].append({'file':case['file'],'expected_rejection':True});continue
        assert case['expected_supported'],case['file']
        length=struct.unpack_from('<I',packet)[0];meta=json.loads(packet[4:4+length]);n=meta['width']*meta['height']
        fmt={8:'B',16:'h',32:'i',64:'q',-32:'f',-64:'d'}[meta['bitpix']]
        size=abs(meta['bitpix'])//8;raw=struct.unpack_from('<'+str(n)+fmt,packet,4+length)
        values=[None if not math.isfinite(v) or (meta['blank'] is not None and meta['bitpix']>0 and v==meta['blank']) else v*meta['bscale']+meta['bzero'] for v in raw]
        assert len(values)==len(case['values'])
        for actual,expected in zip(values,case['values']):
            assert (actual is None and expected is None) or (actual is not None and expected is not None and abs(actual-expected)<=1e-5*max(1,abs(expected))),case['file']
        if case['kind']=='spectrum':
            assert meta['spectrum'] is not None
            coordinates=struct.unpack_from('<'+str(n)+'d',packet,4+length+n*size)
            assert all(abs(a-b)<1e-8 for a,b in zip(coordinates,case['x']))
        report['cases'].append({'file':case['file'],'decoded_values':n,'kind':case['kind']})
    name=next(c['file'] for c in json.loads((data/'reference.json').read_text(encoding='utf-8')) if c['file'].startswith('中文数据/') and c['file'].endswith('.fz'))
    with get(base+'/api/image?'+urllib.parse.urlencode({'path':'"'+name+'"'}),info['token']) as response:assert response.status==200
    for invalid in ['../outside.fits','"../outside.fits"']:
        try:get(base+'/api/image?'+urllib.parse.urlencode({'path':invalid}),info['token'])
        except urllib.error.HTTPError as error:assert error.code==403
        else:raise AssertionError('Parent traversal accepted')
    report['quoted_unicode_path']=True
    report['parent_traversal_rejected']=True
    if args.native:
        if os.name!='nt':
            display_process=subprocess.Popen(['Xvfb','-displayfd','1','-screen','0','1280x900x24','-nolisten','tcp'],stdout=subprocess.PIPE,stderr=log)
            env['DISPLAY']=':'+display_process.stdout.readline().decode().strip()
            env['LIBGL_ALWAYS_SOFTWARE']='1'
        native=[]
        for i,name in enumerate([name,'中文数据/中文图像（测试）.fits','中文数据/中文图像（测试）.fits.gz','spectrum_1d.fits','spectrum_vector.fits','spectrum.txt']):
            state=scratch/f'native-{i}.json';local=dict(env,FITSVIEWER_DIAGNOSTICS=str(state),FITSVIEWER_EXIT_AFTER='2.5')
            p=subprocess.Popen([str(binary),'--native',name],cwd=data,env=local,stdout=log,stderr=subprocess.STDOUT)
            try:
                if args.capture_python and os.name!='nt':
                    wait_json(state,p)
                    time.sleep(.35)
                    capture=scratch/f'native-{i}.png'
                    code='from PIL import ImageGrab; ImageGrab.grab(xdisplay='+repr(env['DISPLAY'])+').save('+repr(str(capture))+')'
                    subprocess.run([str(args.capture_python),'-c',code],env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
                assert p.wait(timeout=40)==0
                snapshot=json.loads(state.read_text(encoding='utf-8'))
                assert snapshot['loaded'] and not snapshot['error'] and (snapshot['texture'] or snapshot['spectrum']),snapshot
                assert snapshot['status_height']==40,snapshot
                native.append({'file':name,'rendered':True,'spectrum':bool(snapshot['spectrum']),'status_height':snapshot['status_height']})
            finally:stop(p)
        report['native']=native
    run([sys.executable,'-m','ipykernel','install','--prefix',sys.prefix,'--name','fitsviewer-candidate','--display-name','FitsViewer candidate'])
    notebook={'nbformat':4,'nbformat_minor':5,'metadata':{'kernelspec':{'name':'fitsviewer-candidate','display_name':'FitsViewer candidate','language':'python'}},'cells':[{'cell_type':'code','id':'candidate-spectrum','metadata':{},'execution_count':None,'outputs':[],'source':['from fitsviewer import show\n',"show('spectrum_1d.fits', height=500)"]}]}
    (data/'demo.ipynb').write_text(json.dumps(notebook),encoding='utf-8')
    port=args.port
    if not port:
        with socket.socket() as sock:sock.bind(('127.0.0.1',0));port=sock.getsockname()[1]
    token=secrets.token_urlsafe(24)
    command=[sys.executable,'-m','jupyterlab','--no-browser','--LabApp.expose_app_in_browser=True','--ServerApp.ip=127.0.0.1',f'--ServerApp.port={port}','--ServerApp.port_retries=0','--ServerApp.base_url=/candidate-test/',f'--ServerApp.root_dir={data}',f'--IdentityProvider.token={token}']
    if os.name!='nt':command.append('--allow-root')
    jupyter=subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT)
    url=f'http://127.0.0.1:{port}/candidate-test/'
    for _ in range(180):
        try:
            with urllib.request.urlopen(url+'api/status?token='+token,timeout=2) as response:break
        except (urllib.error.URLError,TimeoutError):
            if jupyter.poll() is not None:raise RuntimeError('Candidate Jupyter exited')
            time.sleep(.5)
    else:raise TimeoutError('Candidate Jupyter startup')
    config={'url':url,'token':token,'data_root':str(data),'scratch':str(scratch),'rust_pid':server.pid,'jupyter_pid':jupyter.pid}
    (scratch/'browser-config.json').write_text(json.dumps(config),encoding='utf-8')
    report['jupyter_port']=port
    (scratch/'validation.json').write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding='utf-8')
    print(json.dumps({'ready':True,'platform':sys.platform,'cases':len(report['cases']),'jupyter_port':port,'native_cases':len(report.get('native',[]))}),flush=True)
    success=True
finally:
    if display_process:stop(display_process)
    if not (args.keep and success):
        stop(jupyter);stop(server)
    log.close()
