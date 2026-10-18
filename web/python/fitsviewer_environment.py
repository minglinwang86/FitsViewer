"""Probe/install optional dependencies in this worker's selected interpreter only.

仅向当前 worker 环境安装缺失包；先解析安装计划，拒绝替换现有包，不使用 shell。
"""
import importlib.metadata,importlib.util,json,os,re,subprocess,sys,tempfile,time,uuid
from fitsviewer_science import require,clean

DEPENDENCIES={
    'numpy':('numpy',['image arrays']),
    'astropy':('astropy',['FITS/WCS/catalogues/VO']),
    'scipy':('scipy',['local peaks/tracing/matching']),
    'matplotlib':('matplotlib',['figure export/MathText/RGB']),
    'specutils':('specutils',['spectral measurements']),
    'pyvo':('pyvo',['asynchronous TAP/UWS']),
    'astroquery':('astroquery',['SDSS catalogue/CDS cross-match']),
    'astroalign':('astroalign',['Gaia pattern recovery when initial WCS is insufficient']),
    'Pillow':('PIL',['raster figure export through Matplotlib']),
    'scikit-image':('skimage',['straight-line Hough detection']),
    'shapely':('shapely',['region geometry']),
    'requests':('requests',['bounded online service transport']),
}


def canonical(name):return re.sub(r'[-_.]+','-',name).lower()


def installed():
    return {canonical(item.metadata['Name']):item.version for item in importlib.metadata.distributions() if item.metadata.get('Name')}


def dependency_conflicts(before,items):
    """Check reverse requirements too: pip's requested graph can omit existing users."""
    from pip._vendor.packaging.requirements import Requirement
    proposed={canonical(item['metadata']['name']):item['metadata']['version'] for item in items};combined=dict(before,**proposed);conflicts=[]
    requirements=[(distribution.metadata.get('Name','installed package'),distribution.requires or []) for distribution in importlib.metadata.distributions()]
    requirements.extend((item['metadata']['name'],item['metadata'].get('requires_dist',[]) or []) for item in items)
    for owner,entries in requirements:
        for text in entries:
            requirement=Requirement(text)
            if requirement.marker is not None and not requirement.marker.evaluate({'extra':''}):continue
            name=canonical(requirement.name)
            if name not in proposed and canonical(owner) not in proposed:continue
            version=combined.get(name)
            if version is None or (requirement.specifier and not requirement.specifier.contains(version,prereleases=True)):
                conflicts.append({'package':owner,'requires':str(requirement),'proposed_dependency_version':version})
    return conflicts


def run(worker,args,log,params,stage,timeout=180):
    """Capture bounded output, expose task stage and terminate only this subprocess."""
    options={'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {}
    log.write(('$ '+sys.executable+' -m pip '+' '.join(args)+'\n').encode());log.flush()
    identifier=getattr(worker.scope,'job',None)
    def progress():
        if identifier in worker.jobs:
            size=os.fstat(log.fileno()).st_size
            with open(log.name,'rb') as source:source.seek(max(0,size-4096));tail=source.read(4096).decode('utf-8','replace')
            worker.jobs[identifier]['progress']={'stage':stage,'log_bytes':size,'log_tail':tail}
    progress()
    process=subprocess.Popen([sys.executable,'-m','pip',*args],stdin=subprocess.DEVNULL,stdout=log,stderr=subprocess.STDOUT,shell=False,**options)
    deadline=time.monotonic()+timeout;last_progress=0.
    try:
        while process.poll() is None:
            worker.check_cancelled()
            if params.get('_cancel') and params['_cancel'].is_set():raise InterruptedError('Dependency operation cancelled')
            if time.monotonic()>deadline:raise TimeoutError('Dependency '+stage+' exceeded its deadline')
            if os.fstat(log.fileno()).st_size>2*1024*1024:raise ValueError('Dependency log exceeded 2 MiB')
            if time.monotonic()-last_progress>.25:progress();last_progress=time.monotonic()
            time.sleep(.05)
        progress()
        return process.returncode
    finally:
        if process.poll() is None:process.terminate()
        try:process.wait(timeout=3)
        except subprocess.TimeoutExpired:process.kill();process.wait()


def probe(worker,params):
    wanted=params.get('packages',list(DEPENDENCIES))
    if not isinstance(wanted,list) or len(wanted)>len(DEPENDENCIES) or any(name not in DEPENDENCIES for name in wanted):raise ValueError('Select dependency names from the supported package list')
    versions=installed();packages=[]
    # Import in a disposable subprocess: ABI/import failures are different from
    # missing distributions, and loaded extension modules do not pollute worker state.
    # 子进程真实导入可识别二进制冲突，同时保持 worker 科研模块按需加载。
    script="""import importlib,json,sys
items=json.loads(sys.argv[1]);out={}
for name,module in items:
 try:
  importlib.import_module(module);out[name]={'importable':True}
 except BaseException as error:out[name]={'importable':False,'error':type(error).__name__+': '+str(error)[:1000]}
print(json.dumps(out))
"""
    output=None;failure=None
    try:
        options={'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {}
        with tempfile.TemporaryFile(dir=worker.cache) as stdout,tempfile.TemporaryFile(dir=worker.cache) as stderr:
            process=subprocess.Popen([sys.executable,'-c',script,json.dumps([(name,DEPENDENCIES[name][0]) for name in wanted])],stdout=stdout,stderr=stderr,stdin=subprocess.DEVNULL,**options)
            deadline=time.monotonic()+60
            try:
                while process.poll() is None:
                    worker.check_cancelled()
                    if params.get('_cancel') and params['_cancel'].is_set():raise InterruptedError('Dependency probe cancelled')
                    if time.monotonic()>deadline:raise TimeoutError('Dependency import probe exceeded 60 seconds')
                    if max(os.fstat(stdout.fileno()).st_size,os.fstat(stderr.fileno()).st_size)>1024*1024:raise ValueError('Probe output exceeded 1 MiB')
                    time.sleep(.05)
                stdout.seek(0);text=stdout.read(1024*1024).decode('utf-8','replace')
                output=json.loads(text.splitlines()[-1]) if process.returncode==0 and text.splitlines() else None
                if output is None:
                    stderr.seek(max(0,os.fstat(stderr.fileno()).st_size-1000));failure='Import subprocess exited '+str(process.returncode)+': '+stderr.read(1000).decode('utf-8','replace')
            finally:
                if process.poll() is None:process.terminate()
                try:process.wait(timeout=3)
                except subprocess.TimeoutExpired:process.kill();process.wait()
    except (TimeoutError,ValueError) as error:failure=str(error)
    for name in wanted:
        module,purposes=DEPENDENCIES[name];version=versions.get(canonical(name));detail=(output or {}).get(name,{})
        importable=detail.get('importable')
        state='available' if importable else 'import_error' if version is not None else 'missing' if output is not None else 'unknown'
        packages.append({'name':name,'module':module,'version':version,'state':state,'importable':importable,
                         'error':detail.get('error') or failure,'purposes':purposes,'installable':state=='missing'})
    return {'python':sys.executable,'prefix':sys.prefix,'python_version':sys.version.split()[0],'packages':packages,
            'pip_available':importlib.util.find_spec('pip') is not None,'missing':[item['name'] for item in packages if item['state']=='missing'],
            'import_errors':[item['name'] for item in packages if item['state']=='import_error'],'environment_selected_by_host':True,
            'bundled_scientific_packages':False,'install_policy':'missing distributions only; resolve first and reject replacing/upgrading/downgrading existing distributions'}


def install(worker,params):
    wanted=params.get('packages')
    if not isinstance(wanted,list) or not 1<=len(wanted)<=len(DEPENDENCIES) or any(not isinstance(name,str) or name not in DEPENDENCIES for name in wanted):raise ValueError('Install accepts only the supported package names, without versions, URLs or pip options')
    wanted=list(dict.fromkeys(wanted));before=installed();missing=[name for name in wanted if canonical(name) not in before]
    if not missing:return {'status':'already_present','python':sys.executable,'requested':wanted,'installed':[],
                           'message':'Requested distributions already exist; import errors require resolving the environment explicitly, without automatic replacement.'}
    require('pip')
    token='dependencies-'+uuid.uuid4().hex;logpath=worker.cache/(token+'.log');planpath=worker.cache/(token+'.json')
    result={'python':sys.executable,'requested':wanted,'installed':[],'preexisting_packages_preserved':True}
    flags=['--disable-pip-version-check','--no-input']
    try:
        with logpath.open('w+b') as log:
            code=run(worker,['install',*flags,'--dry-run','--report',str(planpath),*missing],log,params,'resolve',180)
            if code or not planpath.is_file():result.update(status='resolution_failed',message='pip could not produce an installation plan; review the log. A pip with --dry-run/--report support is required.')
            else:
                plan=json.loads(planpath.read_text(encoding='utf-8'));items=plan.get('install',[]);replacements=[];specs=[]
                if not isinstance(items,list) or len(items)>100:raise ValueError('Dependency resolver returned an invalid or excessive plan')
                for item in items:
                    metadata=item.get('metadata',{});name=metadata.get('name','');version=metadata.get('version','')
                    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]*',name) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.!+_-]*',version):raise ValueError('Resolver returned an unsafe package name or version')
                    if canonical(name) in before:replacements.append({'name':name,'current':before[canonical(name)],'proposed':version})
                    specs.append(name+'=='+version)
                result['plan']=specs
                if not replacements:replacements=dependency_conflicts(before,items)
                if replacements:result.update(status='blocked_conflict',conflicts=replacements,message='Installation would replace existing packages. No installation was performed; choose a compatible environment or resolve versions explicitly.')
                elif not specs:result.update(status='already_present',message='Resolver reported no missing distributions.')
                elif installed()!=before:result.update(status='blocked_conflict',message='Environment changed during resolution. No installation was performed; detect dependencies again.')
                else:
                    # Freeze the reviewed resolver output; --no-deps prevents the
                    # real install from silently choosing a different dependency graph.
                    # 安装固定计划并禁依赖二次解析，防止静默替换已有依赖。
                    code=run(worker,['install',*flags,'--no-deps',*specs],log,params,'install',600)
                    after=installed();changed=[name for name,version in before.items() if after.get(name)!=version]
                    added=[{'name':name,'version':version} for name,version in after.items() if name not in before]
                    result.update(status='installed' if code==0 and not changed else 'install_failed',installed=added,preexisting_packages_preserved=not changed,changed_existing=changed)
                    check=run(worker,['check',*flags],log,params,'check',60)
                    result['pip_check_passed']=check==0
                    if check:result['message']='Installation finished with dependency conflicts reported by pip check; inspect the log before using the affected tools.'
                    if code:result['message']='Installation failed and may have installed a subset of new packages; inspect the log. Existing packages were not requested for replacement.'
        result['log']=worker.artifact(logpath,'log')
        return clean(result)
    except InterruptedError:
        logpath.unlink(missing_ok=True);raise
    except Exception as error:
        result.update(status='install_failed',message=str(error))
        if logpath.exists():
            if logpath.stat().st_size>2*1024*1024:
                with logpath.open('r+b') as stream:stream.truncate(2*1024*1024)
            result['log']=worker.artifact(logpath,'log')
        return clean(result)
    finally:planpath.unlink(missing_ok=True)
