"""Optional Jupyter binding, executed by the Python chosen in Windows Setup.

由安装界面所选的 Python 执行；独立桌面窗口无需 Python。
"""
from pathlib import Path
import argparse
import hashlib
import importlib.metadata as metadata
import json
import os
import subprocess
import sys

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--wheel',type=Path)
parser.add_argument('--state',required=True,type=Path)
parser.add_argument('--log',type=Path,help='Installation log; defaults to install-jupyter.log beside --state')
parser.add_argument('--default-viewer',choices=['yes','no'],default='yes')
parser.add_argument('--with-jupyter',action='store_true')
parser.add_argument('--unlink',action='store_true')
args=parser.parse_args()
log_path=args.log or args.state.with_name('install-jupyter.log')
log_path.parent.mkdir(parents=True,exist_ok=True)
print('Installation log:',log_path,flush=True)
log=log_path.open('a',encoding='utf-8',buffering=1)
sys.stdout=sys.stderr=log
print('\nFitsViewer environment integration:',sys.executable,flush=True)
if sys.version_info < (3,10): raise SystemExit('Python 3.10 or newer is required.')
environment={k:v for k,v in os.environ.items() if not k.startswith('FITSVIEWER_') and k not in {'PYTHONPATH','PYTHONHOME'}}
environment['PATH']=os.pathsep.join([str(Path(sys.prefix)/p) for p in ['', 'Scripts', 'Library/bin']]+[environment.get('PATH','')])
def run(command):
    print('Running:',subprocess.list2cmdline(command),flush=True)
    subprocess.run(command,env=environment,stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,check=True)
def version(name):
    try: return metadata.version(name)
    except metadata.PackageNotFoundError: return None

if args.unlink:
    record=json.loads(args.state.read_text())
    if record['prefix']!=sys.prefix: raise SystemExit('Environment mismatch; uninstall was not performed.')
    if record['installed_by_setup'] and version('fitsviewer')==record['version']:
        run([sys.executable,'-m','pip','uninstall','-y','fitsviewer'])
        print('The Jupyter integration installed by Setup was removed.')
    else:
        print('Keeping a pre-existing or subsequently changed Jupyter package.')
    raise SystemExit(0)

if not args.wheel or not args.wheel.is_file(): raise SystemExit('The Windows wheel is missing.')
previous=version('fitsviewer')
lab=version('jupyterlab')
if lab and int(lab.split('.')[0])>=5:
    raise SystemExit('JupyterLab 5.x is not supported yet. Select a separate JupyterLab 4 environment; no downgrade was performed.')
if not args.with_jupyter:
    if not lab or tuple(map(int,lab.split('.')[:2])) < (4,4) or lab.split('.')[0]!='4':
        raise SystemExit('Choose JupyterLab 4.4+ (<5), or enable dependency installation in Setup.')
    for name in ['jupyter-server-proxy','IPython']:
        if not version(name): raise SystemExit(name+' is missing; enable dependency installation in Setup.')
command=[sys.executable,'-m','pip','install','--upgrade']
if not args.with_jupyter: command.append('--no-deps')
run([*command,str(args.wheel)])
# A fresh subprocess sees newly installed package files without import caches.
# 使用新的解释器进程读取新安装的模块，避免复用安装前的导入缓存。
for name in ['fitsviewer','jupyter_server_proxy']:
    run([sys.executable,'-m','jupyter','server','extension','enable','--py',name,'--sys-prefix'])
run([sys.executable,'-m','jupyter','labextension','enable','fitsviewer','fitsviewer:viewer','--level=sys_prefix'])
code="from fitsviewer.settings import save_settings; save_settings({'default_viewer': "+str(args.default_viewer=='yes')+"})"
run([sys.executable,'-c',code])
installed=metadata.version('fitsviewer')
record={'python':sys.executable,'prefix':sys.prefix,'version':installed,'installed_by_setup':previous is None,
        'wheel_sha256':hashlib.sha256(args.wheel.read_bytes()).hexdigest()}
if args.state.exists():
    old=json.loads(args.state.read_text())
    if old.get('prefix')==sys.prefix and old.get('installed_by_setup'):
        record['installed_by_setup']=True
args.state.parent.mkdir(parents=True,exist_ok=True)
args.state.write_text(json.dumps(record,indent=2)+'\n')
print('FitsViewer Jupyter integration installed successfully.')
print('Installation environment:',sys.prefix)
print('Python executable:',sys.executable)
print('Default FITS viewer in JupyterLab:',args.default_viewer)
print('Restart JupyterLab from this environment and refresh the browser.')
