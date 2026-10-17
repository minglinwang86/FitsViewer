"""Verify a Windows wheel in a dedicated, dependency-ready Python environment.

独立环境验证 Windows 原生渲染、后端、Jupyter 文件双击和 Notebook 内嵌；日志仅本地保存。
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile
import psutil

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--wheel', required=True, type=Path)
parser.add_argument('--python', required=True, type=Path, help='Dedicated test environment with wheel dependencies installed')
parser.add_argument('--scratch', required=True, type=Path, help='New test directory; never reuse production state')
parser.add_argument('--skip-native', action='store_true', help='Skip visible, automatically closing native windows')
args = parser.parse_args()
assert os.name == 'nt', 'Run on Windows'
scratch = args.scratch.resolve()
assert not scratch.exists(), 'Select a new test directory'
scratch.mkdir(parents=True)
wheel = args.wheel.resolve()
python = str(args.python.resolve())
environment = {k: v for k, v in os.environ.items() if not k.startswith(('FITSVIEWER_', 'JUPYTER_', 'PYTHON'))}
environment.update(FITSVIEWER_RUNTIME_DIR=str(scratch/'viewer-runtime'), FITSVIEWER_CONFIG=str(scratch/'settings.json'),
                   JUPYTER_CONFIG_DIR=str(scratch/'config'), JUPYTER_DATA_DIR=str(scratch/'data'),
                   JUPYTER_RUNTIME_DIR=str(scratch/'runtime'), JUPYTER_PREFER_ENV_PATH='1',
                   XDG_CACHE_HOME=str(scratch/'cache'), XDG_CONFIG_HOME=str(scratch/'preferences'),
                   PYTHONIOENCODING='utf-8')
log = (scratch/'test.log').open('w', encoding='utf-8')
def run(command, **options):
    return subprocess.run([str(p) for p in command], env=environment, stdin=subprocess.DEVNULL,
                          check=True, **options)
def output(command):
    return run(command, stdout=subprocess.PIPE, stderr=log, text=True, encoding='utf-8').stdout
def stop(process):
    if process and process.poll() is None:
        try:
            descendants=psutil.Process(process.pid).children(recursive=True)
            for child in reversed(descendants):
                try: child.terminate()
                except psutil.NoSuchProcess: pass
        except psutil.NoSuchProcess: pass
        process.terminate()
        try: process.wait(timeout=10)
        except subprocess.TimeoutExpired: process.kill(); process.wait()
def wait_json(file, process):
    for _ in range(180):
        candidates=list(file.parent.glob('jpserver-*.json')) if file.name.startswith('jpserver-') else [file]
        for candidate in candidates:
            if not candidate.is_file(): continue
            try: return candidate, json.loads(candidate.read_text())
            except (json.JSONDecodeError, PermissionError): pass
        if process.poll() is not None: raise RuntimeError('Process exited before startup; inspect '+str(scratch))
        time.sleep(.25)
    raise TimeoutError(str(file))
def get(url, headers=None):
    return urllib.request.urlopen(urllib.request.Request(url, headers=headers or {}), timeout=45)
report = {'wheel':wheel.name, 'sha256':hashlib.sha256(wheel.read_bytes()).hexdigest()}
with zipfile.ZipFile(wheel) as archive:
    entry=next(n for n in archive.namelist() if n.endswith('.dist-info/entry_points.txt'))
    entries=archive.read(entry).decode()
    assert 'fitsviewer = fitsviewer.cli:main' in entries
    assert 'fitsview =' not in entries and 'fitsviewer-jupyter' not in entries
    assert not any(n.startswith(('fitsview_jupyter/','fitsviewer_jupyter/')) for n in archive.namelist())
report['names_without_aliases']=True
run([python,'-m','pip','install','--force-reinstall','--no-deps',wheel], stdout=log, stderr=subprocess.STDOUT)
run([python,'-m','pip','check'], stdout=log, stderr=subprocess.STDOUT)
info = json.loads(output([python,'-c',
    'import json,sys,jupyterlab; from fitsviewer.runtime import resources; '
    'print(json.dumps(dict(python=sys.version.split()[0],jupyterlab=jupyterlab.__version__,prefix=sys.prefix,resources=[str(p) for p in resources()])))']))
report.update(python=info['python'],jupyterlab=info['jupyterlab'],pip_check=True)
vendor,binary,web = map(Path,info['resources'])
import ctypes
shell=ctypes.WinDLL('shell32',use_last_error=True)
shell.ExtractIconExW.argtypes=[ctypes.c_wchar_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_void_p,ctypes.c_uint]
shell.ExtractIconExW.restype=ctypes.c_uint
report['embedded_icon_groups']=shell.ExtractIconExW(str(binary),-1,None,None,0)
assert report['embedded_icon_groups']>0
cli = Path(info['prefix'])/'Scripts/fitsviewer.exe'
data = scratch/'synthetic data'
data.mkdir()
for name in ['science.fits','second.fits','Unicode 星场.fits']:
    shutil.copy2(root/'examples/starfield.fits',data/name)
native = []
if not args.skip_native:
    for index,name in enumerate(['science.fits','Unicode 星场.fits','science.fits']):
        diagnostics = scratch/f'native-{index}.json'
        env = dict(environment,FITSVIEWER_DIAGNOSTICS=str(diagnostics),FITSVIEWER_EXIT_AFTER='3')
        process = subprocess.Popen([str(binary),'--native',str(data/name)],env=env,stdout=log,stderr=subprocess.STDOUT)
        try:
            assert process.wait(timeout=30)==0
            state=json.loads(diagnostics.read_text())
            assert state['loaded'] and state['texture'] and not state['error'] and state['width']==512, state
            native.append({'loaded':True,'texture':True,'exit_code':0,'unicode_path':index==1})
        finally: stop(process)
report['native_render_close_cycles']=native
server = None
jupyter = None
proxy = None
try:
    server=subprocess.Popen([str(binary),'--serve','--port','0','--root',str(data)],env=environment,stdout=log,stderr=subprocess.STDOUT)
    _,runtime=wait_json(scratch/'viewer-runtime/server.json',server)
    base=f"http://127.0.0.1:{runtime['port']}"
    assert json.load(get(base+'/api/health'))['viewer']=='fitsviewer'
    query=urllib.parse.urlencode({'path':'science.fits','token':runtime['token']})
    with get(base+'/api/image?'+query) as response: assert len(response.read())>512*512
    try: get(base+'/api/image?path=science.fits')
    except urllib.error.HTTPError as error: assert error.code==401
    else: raise AssertionError('Unauthenticated image access succeeded')
    stop(server); server=None
    (scratch/'viewer-runtime/server.json').unlink()
    report['backend']={'auto_port':True,'fits_api':True,'authentication':True}
    run([cli,'ports','set','--browser','auto','--proxy','auto'],stdout=log,stderr=subprocess.STDOUT)
    run([python,'-m','ipykernel','install','--sys-prefix','--name','fitsviewer-wheel-test'],stdout=log,stderr=subprocess.STDOUT)
    notebook={'nbformat':4,'nbformat_minor':5,'metadata':{'kernelspec':{'name':'fitsviewer-wheel-test','display_name':'FitsViewer test','language':'python'}},
              'cells':[{'cell_type':'code','id':'viewer','metadata':{},'execution_count':None,'outputs':[],
                        'source':['from fitsviewer import show\n',"show('science.fits')"]}]}
    (data/'demo.ipynb').write_text(json.dumps(notebook))
    with socket.socket() as probe: probe.bind(('127.0.0.1',0)); port=probe.getsockname()[1]
    command=[python,'-m','jupyterlab','--no-browser','--ip=127.0.0.1',f'--port={port}',
             '--ServerApp.port_retries=0','--ServerApp.base_url=/wheel-test/',
             '--LabApp.expose_app_in_browser=True',f'--ServerApp.root_dir={data}']
    jupyter=subprocess.Popen(command,env=environment,cwd=data,stdout=log,stderr=subprocess.STDOUT)
    info_file=scratch/f'runtime/jpserver-{jupyter.pid}.json'
    info_file,jupyter_info=wait_json(info_file,jupyter)
    opened=output([cli,data/'science.fits','--no-browser'])
    assert '/wheel-test/fitsviewer/?path=' in opened, opened
    _,proxy_record=wait_json(scratch/'viewer-runtime/server.json',jupyter)
    proxy=psutil.Process(proxy_record['pid'])
    assert Path(proxy.exe()).resolve()==binary.resolve()
    assert jupyter.poll() is None
    report['cli_jupyter_warmup']=True
    config={'server_info':str(info_file),'venv':info['prefix'],'data_root':str(data),'environment':environment}
    config_file=scratch/'browser.json'
    config_file.write_text(json.dumps(config))
    browser=output([shutil.which('node'),root/'scripts/verify_wheel.cjs',config_file])
    report['browser']=json.loads(browser.strip().splitlines()[-1])
    # Close kernels before stopping Jupyter; only processes from this test are touched.
    headers={'Authorization':'token '+jupyter_info.get('token','')}
    with get(jupyter_info['url']+'api/sessions',headers) as reply: sessions=json.load(reply)
    for session in sessions:
        urllib.request.urlopen(urllib.request.Request(jupyter_info['url']+'api/sessions/'+session['id'],headers=headers,method='DELETE')).close()
    report['passed']=True
finally:
    stop(jupyter)
    stop(server)
    if proxy:
        try:
            proxy.wait(timeout=10)
        except psutil.TimeoutExpired:
            if Path(proxy.exe()).resolve()==binary.resolve(): proxy.terminate(); proxy.wait(timeout=10)
        except psutil.NoSuchProcess: pass
    (scratch/'validation.json').write_text(json.dumps(report,indent=2)+'\n')
    log.close()
print(json.dumps(report))
