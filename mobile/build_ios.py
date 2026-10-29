"""Build an unsigned iOS device app or an ad-hoc signed simulator app on macOS."""
import argparse
from pathlib import Path
import plistlib
import shutil
import subprocess
import tempfile
from verify_assets import verify

ROOT=Path(__file__).resolve().parents[1]

def run(argv):
    subprocess.run([str(arg) for arg in argv],check=True)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--assets',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--simulator',action='store_true')
    args=parser.parse_args()
    assets=args.assets.resolve()
    metadata=verify(assets)
    app=args.output.resolve()/'FitsViewerMini.app'
    if app.exists():
        raise FileExistsError('Use a new build output directory')
    app.mkdir(parents=True)
    sdk='iphonesimulator' if args.simulator else 'iphoneos'
    sdk_path=subprocess.check_output(['xcrun','--sdk',sdk,'--show-sdk-path'],text=True).strip()
    architecture='arm64' if __import__('platform').machine()=='arm64' or not args.simulator else 'x86_64'
    target=architecture+'-apple-ios15.0'+('-simulator' if args.simulator else '')
    run(['xcrun','--sdk',sdk,'swiftc','-parse-as-library','-O','-target',target,'-sdk',sdk_path,'-Xlinker','-no_adhoc_codesign',
         '-framework','UIKit','-framework','WebKit',ROOT/'mobile/ios/AppDelegate.swift','-o',app/'FitsViewerMini'])
    info=plistlib.loads((ROOT/'mobile/ios/Info.plist').read_bytes())
    info.update(CFBundleIdentifier='org.fitsviewer.mini',CFBundleExecutable='FitsViewerMini',CFBundleName='FitsViewerMini',
                CFBundleSupportedPlatforms=['iPhoneSimulator' if args.simulator else 'iPhoneOS'],MinimumOSVersion='15.0',UIDeviceFamily=[1,2])
    site=app/'site';site.mkdir()
    for name in [*metadata['files'],'build-profile.json']:
        shutil.copy2(assets/name,site/name)
    # The application icon belongs to the app only. No FITS file association is registered.
    with tempfile.TemporaryDirectory(prefix='fitsviewer-ios-icons-') as temporary:
        catalog=Path(temporary)/'Assets.xcassets';icons=catalog/'AppIcon.appiconset';icons.mkdir(parents=True)
        images=[]
        for idiom,size,scale in [('iphone',20,2),('iphone',20,3),('iphone',29,2),('iphone',29,3),('iphone',40,2),('iphone',40,3),('iphone',60,2),('iphone',60,3),('ipad',20,1),('ipad',20,2),('ipad',29,1),('ipad',29,2),('ipad',40,1),('ipad',40,2),('ipad',76,1),('ipad',76,2),('ipad',83.5,2),('ios-marketing',1024,1)]:
            name=f'{idiom}-{size}-{scale}.png';pixels=int(size*scale)
            run(['sips','-z',pixels,pixels,assets/'icon.png','--out',icons/name])
            images.append({'idiom':idiom,'size':f'{size}x{size}','scale':f'{scale}x','filename':name})
        import json
        (icons/'Contents.json').write_text(json.dumps({'images':images,'info':{'version':1,'author':'fitsviewer'}}))
        partial=Path(temporary)/'icons.plist'
        run(['xcrun','actool',catalog,'--compile',app,'--platform',sdk,'--minimum-deployment-target','15.0','--app-icon','AppIcon','--output-partial-info-plist',partial,'--target-device','iphone','--target-device','ipad'])
        info.update(plistlib.loads(partial.read_bytes()))
    (app/'Info.plist').write_bytes(plistlib.dumps(info))
    (app/'PkgInfo').write_bytes(b'APPL????')
    if args.simulator:
        run(['codesign','--force','--sign','-',app])
    else:
        print('UNSIGNED DEVICE APP: cannot be installed until valid Apple development/distribution signing and provisioning are supplied.')
    print(app)

if __name__=='__main__':
    main()
