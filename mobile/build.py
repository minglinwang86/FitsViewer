"""Build the shared viewer as base-only offline web assets for mobile shells."""
import argparse
import base64
import hashlib
import html as html_escape
import importlib.util
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=ROOT)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target-dir', type=Path, required=True)
    parser.add_argument('--execute', action='store_true')
    args = parser.parse_args()
    source, output, target = args.source.resolve(), args.output.resolve(), args.target_dir.resolve()
    spec = importlib.util.spec_from_file_location('build_profiles', source/'scripts/build_profiles.py')
    profiles = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = profiles
    spec.loader.exec_module(profiles)
    selection = profiles.select(modules=['base'])
    assert selection.viewer_features == [] and selection.modules == ('base',)
    command = profiles.cargo_arguments('fitsviewer-web', selection.viewer_features)
    command += ['--features', 'offline', '--target', 'wasm32-unknown-unknown', '--target-dir', str(target)]
    plan = {**selection.metadata(), 'wasm_features':['offline'], 'cargo':command, 'will_execute':args.execute,
            'payload':'shared core and viewer; no host, Python, Jupyter, or PRO', 'input_limit_bytes':64*1024*1024}
    print(json.dumps(plan, indent=2))
    if not args.execute:
        return
    output.mkdir(parents=True, exist_ok=True)
    subprocess.run(command, cwd=source, check=True)
    with tempfile.TemporaryDirectory(prefix='fitsviewer-mini-bindgen-') as temporary:
        pkg = Path(temporary)
        subprocess.run(['wasm-bindgen', '--target', 'web', '--no-typescript', '--out-dir', str(pkg),
                        str(target/'wasm32-unknown-unknown/release/fitsviewer_web.wasm')], check=True)
        subprocess.run([sys.executable, str(source/'scripts/build_static.py'), '--pkg', str(pkg), '--output', str(output/'index.html')], check=True)
    html = (output/'index.html').read_text(encoding='utf-8')
    html = html.replace('FitsViewer offline','FitsViewer Mini').replace('FitsViewer 离线查看器','FitsViewer Mini')
    html = html.replace('64 MiB input/decoded limit.', '64 MiB file / gzip-expanded input limit. Decoded images also depend on device memory.')
    html = html.replace('输入及解压上限 64 MiB。', '文件及 gzip 展开输入上限 64 MiB；解码图像还受设备内存限制。')
    html = html.replace('width=device-width,initial-scale=1', 'width=device-width,initial-scale=1,viewport-fit=cover')
    html = html.replace('<title>', '<link rel="manifest" href="manifest.webmanifest"><meta name="theme-color" content="#0d1117"><meta name="apple-mobile-web-app-capable" content="yes"><link rel="apple-touch-icon" href="icon.png"><title>', 1)
    html = html.replace('</style>', '\n'+(ROOT/'mobile/shell.css').read_text(encoding='utf-8')+'\n</style>', 1)
    html = html.replace('<button id="close">', '<select id="destination" aria-label="Open destination"><option value="replace">Replace current</option><option value="tab">Open in new tab</option></select><button id="zoom-out" aria-label="Zoom out">−</button><button id="zoom-in" aria-label="Zoom in">+</button><button id="fit">Fit</button><button id="close">', 1)
    dictionaries={entry['code']:json.loads((source/'web/locales'/f"{entry['code']}.json").read_text()) for entry in json.loads((source/'web/locales/registry.json').read_text())['languages']}
    keys=['Replace current','Open in new tab','Fit']
    html = html.replace('<select id="destination"', '<script type="application/json" id="mobile-shared-labels">'+json.dumps({code:{key:value.get(key,key) for key in keys} for code,value in dictionaries.items()},ensure_ascii=False).replace('</','<\\/')+'</script><select id="destination"',1)
    # Unknown FITS extensions must remain selectable in iOS Files and Android providers.
    html = html.replace('accept=".fits,.fit,.fts,.fz,.gz,.txt,.dat,.csv,.ascii"', 'accept="*/*"')
    html = html.replace('</body>', '<script>'+ (ROOT/'mobile/shell.js').read_text(encoding='utf-8')+'</script></body>') if '</body>' in html else html.replace('</html>', '<script>'+(ROOT/'mobile/shell.js').read_text(encoding='utf-8')+'</script></html>')
    mobile_notice=(ROOT/'mobile/ANDROID_NOTICE.txt').read_text(encoding='utf-8')
    html = html.replace('</pre>', '\n\n'+html_escape.escape(mobile_notice)+'</pre>', 1)
    embedded=[base64.b64decode(value) for value in re.findall(r"bytes\('([A-Za-z0-9+/=]+)'\)",html)]
    wasm=next(value for value in embedded if value.startswith(b'\0asm'))
    font=(source/'web/fonts/cjk.ttf').read_bytes()
    if font not in embedded:
        raise ValueError('The shared CJK font was not embedded')
    plan['inline_resources']={name:{'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()} for name,data in [('wasm',wasm),('cjk_font',font)]}
    license_inventory={}
    for path in [source/'LICENSE',source/'NOTICE.md',source/'THIRD_PARTY_NOTICES.md',*sorted((source/'licenses').rglob('*'))]:
        if path.is_file() and path.suffix not in ['.json','.png','.ttf']:
            text=path.read_text(encoding='utf-8',errors='replace')
            if html_escape.escape(text) not in html:
                raise ValueError('Missing embedded license: '+str(path.relative_to(source)))
            license_inventory[path.relative_to(source).as_posix()]=hashlib.sha256(text.encode()).hexdigest()
    plan['embedded_license_inventory']=license_inventory
    plan['embedded_license_inventory']['mobile/ANDROID_NOTICE.txt']=hashlib.sha256(mobile_notice.encode()).hexdigest()
    (output/'index.html').write_text(html, encoding='utf-8')
    shutil.copy2(source/'web/icons/fitsviewer.png', output/'icon.png')
    for size in [192,512]:
        shutil.copy2(ROOT/f'mobile/icons/icon-{size}.png', output/f'icon-{size}.png')
    shutil.copy2(ROOT/'mobile/manifest.webmanifest', output/'manifest.webmanifest')
    digest = hashlib.sha256(html.encode()).hexdigest()[:16]
    (output/'sw.js').write_text((ROOT/'mobile/sw.js').read_text().replace('__BUILD__', digest), encoding='utf-8')
    names=['index.html','icon.png','icon-192.png','icon-512.png','manifest.webmanifest','sw.js']
    plan['files'] = {name:{'bytes':(output/name).stat().st_size,'sha256':hashlib.sha256((output/name).read_bytes()).hexdigest()} for name in names}
    plan['source_commit'] = os.environ.get('GITHUB_SHA')
    tracked=[source/'Cargo.toml',source/'Cargo.lock',source/'config/build-modules.json',*sorted((source/'crates/core').rglob('*.rs')),*sorted((source/'crates/viewer').rglob('*.rs'))]
    plan['source_hashes']={p.relative_to(source).as_posix():hashlib.sha256(p.read_bytes()).hexdigest() for p in tracked}
    (output/'build-profile.json').write_text(json.dumps(plan, indent=2)+'\n', encoding='utf-8')

if __name__ == '__main__':
    main()
