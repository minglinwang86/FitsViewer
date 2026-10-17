"""Exercise silent Windows Setup using dedicated venv/Conda targets.

验证安装选项和卸载恢复；运行前必须没有已注册的 FitsViewer 桌面安装。
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import subprocess
import time
import winreg

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--setup',required=True,type=Path)
parser.add_argument('--python',required=True,type=Path)
parser.add_argument('--conda-prefix',required=True,type=Path)
parser.add_argument('--scratch',required=True,type=Path)
parser.add_argument('--payload',type=Path,help='Optional source payload for installed-file integrity checks')
args=parser.parse_args()
assert os.name=='nt'
scratch=args.scratch.resolve()
assert not scratch.exists()
scratch.mkdir(parents=True)
setup=args.setup.resolve()
app=scratch/'installed app'
env={k:v for k,v in os.environ.items() if not k.startswith(('FITSVIEWER_','JUPYTER_','PYTHON'))}
env.update(XDG_CONFIG_HOME=str(scratch/'preferences'),XDG_CACHE_HOME=str(scratch/'cache'),
           JUPYTER_CONFIG_DIR=str(scratch/'jupyter-config'),JUPYTER_DATA_DIR=str(scratch/'jupyter-data'),
           JUPYTER_RUNTIME_DIR=str(scratch/'runtime'),PYTHONIOENCODING='utf-8')
def run(command,check=True):
    return subprocess.run([str(p) for p in command],env=env,stdin=subprocess.DEVNULL,
                          stdout=subprocess.PIPE,stderr=subprocess.STDOUT,encoding='utf-8',errors='replace',
                          timeout=900,check=check)
def value(key,name=''):
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER,key) as handle: return winreg.QueryValueEx(handle,name)[0]
    except FileNotFoundError: return None
def defaults():
    return {e:{'fallback':value('Software\\Classes\\'+e),
               'protected':value('Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\'+e+'\\UserChoice','ProgId')}
            for e in ['.fits','.fit','.fts','.gz']}
assert value('Software\\RegisteredApplications','FitsViewer') is None, 'Use a Windows account without an existing FitsViewer registration'
before=defaults()
report={}
def install(label,options,check=True):
    result=run([setup,'/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/DIR='+str(app),
                '/LOG='+str(scratch/(label+'.log')),*options],check=check)
    report[label]={'exit_code':result.returncode}
    return result
def uninstall():
    run([app/'unins000.exe','/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/LOG='+str(scratch/'uninstall.log')])
    for _ in range(100):
        if not (app/'unins000.exe').exists(): break
        time.sleep(.1)
    assert defaults()==before, 'Previous defaults were not restored'
    assert value('Software\\RegisteredApplications','FitsViewer') is None
    assert value('Software\\Classes\\FitsViewer.FITS\\shell\\open\\command') is None
    for e in ['.fits','.fit','.fts']:
        assert value('Software\\Classes\\'+e+'\\OpenWithProgids','FitsViewer.FITS') is None
    report['uninstall_restores_defaults']=True
def inspect(python):
    prefix=python.parent.parent if python.parent.name.lower()=='scripts' else python.parent
    selected=dict(env,PATH=os.pathsep.join([str(prefix/p) for p in ['', 'Scripts','Library/bin']]+[env.get('PATH','')]))
    code='import json,sys; from fitsviewer.install import installation_details; print(json.dumps(installation_details()))'
    result=subprocess.run([str(python),'-c',code],env=selected,capture_output=True,text=True,encoding='utf-8',check=True)
    return json.loads(result.stdout)
try:
    result=install('invalid-environment',['/TASKS=','/PYTHON='+str(scratch/'missing.exe')],check=False)
    assert result.returncode!=0 and not (app/'fitsviewer.exe').exists()
    future=scratch/'jupyter5-metadata-env'
    run([args.python.resolve(),'-m','venv',future])
    site=future/'Lib/site-packages'
    (site/'jupyterlab').mkdir()
    (site/'jupyterlab/__init__.py').write_text('')
    fake=site/'jupyterlab-5.0.0.dist-info'
    fake.mkdir()
    (fake/'METADATA').write_text('Metadata-Version: 2.4\nName: jupyterlab\nVersion: 5.0.0\n')
    result=install('jupyter5-downgrade-blocked',['/TASKS=','/PYTHON='+str(future/'Scripts/python.exe'),'/WITHJUPYTER=yes'],check=False)
    assert result.returncode!=0 and not (app/'fitsviewer.exe').exists()
    assert 'Version: 5.0.0' in (fake/'METADATA').read_text()
    install('desktop-only',['/TASKS='])
    assert (app/'fitsviewer.exe').is_file()
    if args.payload:
        expected = [p for p in args.payload.resolve().rglob('*') if p.is_file()]
        for file in expected:
            installed = app / file.relative_to(args.payload.resolve())
            assert installed.is_file(), 'Missing installed payload: ' + str(file.relative_to(args.payload.resolve()))
            assert hashlib.sha256(installed.read_bytes()).digest() == hashlib.sha256(file.read_bytes()).digest()
        report['installed_payload_integrity'] = {'files':len(expected),'sha256_matches':True}
    assert defaults()==before
    status=json.loads(run([app/'fitsviewer.exe','--desktop-default','status']).stdout)
    assert status['registered'] and not status['default_requested']
    uninstall()
    install('python-environment',['/TASKS=associate','/PYTHON='+str(args.python.resolve()),'/WITHJUPYTER=no','/JUPYTERDEFAULT=no'])
    status=json.loads(run([app/'fitsviewer.exe','--desktop-default','status']).stdout)
    assert status['default_requested']
    assert status['default'] or status['needs_windows_confirmation']
    after=defaults()
    assert all(after[e]['protected']==before[e]['protected'] for e in after)
    assert after['.gz']==before['.gz']
    details=inspect(args.python.resolve())
    assert details['binding']['enabled'] and not details['state']['configured']['default_viewer']
    run([app/'fitsviewer.exe','--desktop-default','no'])
    assert defaults()==before
    report['default_toggle_and_protected_choices']=True
    uninstall()
    install('conda-environment',['/TASKS=','/CONDAENV='+str(args.conda_prefix.resolve()),'/WITHJUPYTER=yes','/JUPYTERDEFAULT=yes'])
    details=inspect(args.conda_prefix.resolve()/'python.exe')
    assert details['binding']['enabled'] and details['state']['configured']['default_viewer']
    assert Path(details['state']['prefix']).resolve()==args.conda_prefix.resolve()
    uninstall()
    report['passed']=True
finally:
    if value('Software\\RegisteredApplications','FitsViewer') is not None and (app/'unins000.exe').exists(): uninstall()
    (scratch/'validation.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
