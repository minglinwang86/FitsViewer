"""Assemble a Windows portable archive and optional Inno Setup installer.

从已构建的 Windows wheel 整理独立桌面程序；不修改当前 Python 或文件关联。
"""
from pathlib import Path
import argparse
import hashlib
import re
import shutil
import subprocess
import tomllib
import zipfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--wheel', required=True, type=Path)
parser.add_argument('--payload', required=True, type=Path, help='New directory outside the source tree')
parser.add_argument('--out', type=Path)
parser.add_argument('--iscc', type=Path, help='Optional path to the Inno Setup 6 compiler (Windows only)')
args = parser.parse_args()
version = tomllib.loads((root / 'jupyter/pyproject.toml').read_text())['project']['version']
numeric = re.fullmatch(r'(\d+\.\d+\.\d+)(?:rc(\d+))?', version)
if not numeric:
    raise ValueError('Windows PE version requires a three-part stable or rc package version')
numeric_version = numeric[1] + '.' + (numeric[2] or '0')
wheel = args.wheel.resolve()
payload = args.payload.resolve()
assert not payload.is_relative_to(root), 'Keep assembled payloads outside the source tree'
assert wheel.name == f'fitsviewer-{version}-py3-none-win_amd64.whl', 'Select the matching Windows wheel'
assert not payload.exists(), 'Choose a new payload directory; existing contents are never deleted'
out = (args.out or root / 'release' / version).resolve()
out.mkdir(parents=True, exist_ok=True)
payload.mkdir(parents=True)
with zipfile.ZipFile(wheel) as archive:
    assert archive.testzip() is None
    for name in archive.namelist():
        relative = None
        vendor = 'fitsviewer/_vendor/'
        if name.startswith(vendor):
            relative = Path(name[len(vendor):])
            if relative.parts[0] == 'bin': relative = Path(*relative.parts[1:])
        elif '.dist-info/licenses/' in name:
            relative = Path(name.split('.dist-info/licenses/', 1)[1])
        if relative is not None and relative.parts and not name.endswith('/'):
            assert not relative.is_absolute() and '..' not in relative.parts
            target = payload / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.read(name))
assert (payload / 'fitsviewer.exe').read_bytes()[:2] == b'MZ'
assert (payload / 'web/pkg/fitsviewer_web_bg.wasm').is_file()
for name in ['README.md', 'README.zh-CN.md']:
    shutil.copy2(root / name, payload / name)
shutil.copytree(root / 'docs', payload / 'docs')
shutil.copytree(root / 'examples', payload / 'examples', ignore=shutil.ignore_patterns('__pycache__', '.ipynb_checkpoints'))
(payload / 'windows').mkdir()
shutil.copy2(root / 'windows/link_environment.py', payload / 'windows/link_environment.py')
shutil.copy2(root / 'windows/INNO-NOTICE.txt', payload / 'windows/INNO-NOTICE.txt')
shutil.copy2(wheel, payload / wheel.name)
portable = out / f'FitsViewer-{version}-windows-x86_64-portable.zip'
with zipfile.ZipFile(portable, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
    for file in sorted(payload.rglob('*')):
        if file.is_file():
            archive.write(file, arcname=portable.stem + '/' + file.relative_to(payload).as_posix())
assets = [portable, wheel]
if args.iscc:
    subprocess.run([str(args.iscc.resolve()), '/Qp', '/DAppVersion=' + version,
                    '/DAppNumericVersion=' + numeric_version,
                    '/DPayloadDir=' + str(payload), '/DReleaseDir=' + str(out),
                    str(root / 'windows/FitsViewer.iss')], check=True)
    assets.append(out / f'FitsViewer-{version}-windows-x86_64-setup.exe')
for asset in assets:
    print(hashlib.sha256(asset.read_bytes()).hexdigest(), asset.name)
