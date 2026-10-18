"""macOS association uses the installed app bundle and an optional user-installed duti.
macOS 文件关联复用已安装的 .app；自动默认值修改仅调用用户已有 duti。
"""
import json,os,plistlib,shutil,subprocess
from pathlib import Path
IDENTIFIER='org.fitsviewer.viewer'
EXTENSIONS=['fits','fit','fts','fz']
REGISTER=Path('/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister')
STATE=Path.home()/'Library/Application Support/FitsViewer/desktop.json'
def app():return Path(os.environ.get('FITSVIEWER_APP',str(Path.home()/'Applications/FitsViewer.app')))
def defaults():
    result={}
    if shutil.which('duti'):
        for extension in EXTENSIONS:
            try:
                process=subprocess.run(['duti','-x',extension],capture_output=True,text=True,timeout=5);lines=process.stdout.splitlines();result[extension]=lines[-1] if process.returncode==0 and len(lines)>=3 else None
            except subprocess.TimeoutExpired:result[extension]=None
    return result
def icon_details(bundle):
    """Inspect the existing app without changing its resources or code signature.
    仅读取既有 app 的图标声明，不修改资源或代码签名。
    """
    result={'present':False}
    try:
        with (bundle/'Contents/Info.plist').open('rb') as stream:info=plistlib.load(stream)
        if not isinstance(info,dict):raise ValueError('Info.plist is not a dictionary')
        filename=info.get('CFBundleIconFile')
        if isinstance(filename,str) and filename and Path(filename).name==filename:
            path=bundle/'Contents/Resources'/filename
            if not path.suffix:path=path.with_suffix('.icns')
            result.update(icon_file=str(path),present=path.is_file() and path.stat().st_size>0)
        # App bundles may instead declare an icon from their compiled asset catalog.
        # app 也可通过已编译的资源目录声明图标。
        name=info.get('CFBundleIconName')
        if not result['present'] and isinstance(name,str) and name:
            path=bundle/'Contents/Resources/Assets.car'
            result.update(icon_name=name,icon_file=str(path),present=path.is_file() and path.stat().st_size>0)
    except (OSError,ValueError,plistlib.InvalidFileException) as error:
        result['error']=str(error)
    if not result['present']:
        result['repair_hint']='Reinstall a correctly packaged FitsViewer.app with CFBundleIconFile=FitsViewer.icns in Contents/Info.plist and a valid Contents/Resources/FitsViewer.icns; existing app bundles are never modified by desktop registration.'
        reference=Path(__file__).with_name('_vendor')/'web/icons/fitsviewer.icns'
        if reference.is_file():result['reference_icon']=str(reference)
    return result

def details():return {'registered':app().is_dir(),'app_bundle':str(app()),'icon':icon_details(app()),'defaults':defaults(),'automatic_default_tool':shutil.which('duti'),'platform_validation':'requires real macOS verification'}
def install(default=True):
    if not app().is_dir():raise RuntimeError('Install FitsViewer.app in ~/Applications first, or set FITSVIEWER_APP')
    if not REGISTER.is_file():raise RuntimeError('LaunchServices registration tool is unavailable')
    previous=defaults()
    subprocess.run([str(REGISTER),'-f',str(app())],check=True,capture_output=True,timeout=20)
    if default:
        if not shutil.which('duti'):raise RuntimeError('App registered. To set the default, use Finder > Get Info > Open With > Change All, or install duti and rerun fitsviewer defaults set --desktop yes')
        if not STATE.is_file():STATE.parent.mkdir(parents=True,exist_ok=True);STATE.write_text(json.dumps({'previous':previous}),encoding='utf-8')
        for extension in EXTENSIONS:subprocess.run(['duti','-s',IDENTIFIER,extension,'all'],check=True,capture_output=True,timeout=5)
    return details()
def disable_default():
    previous=json.loads(STATE.read_text(encoding='utf-8')).get('previous',{}) if STATE.is_file() else {}
    current=defaults()
    if previous and not shutil.which('duti'):raise RuntimeError('Restore defaults using Finder, or install duti')
    for extension,identifier in previous.items():
        if identifier and identifier!=IDENTIFIER and current.get(extension)==IDENTIFIER:subprocess.run(['duti','-s',identifier,extension,'all'],check=True,capture_output=True,timeout=5)
    return details()
def remove():
    disable_default()
    if REGISTER.is_file() and app().is_dir():subprocess.run([str(REGISTER),'-u',str(app())],check=True,capture_output=True,timeout=20)
    return details()

