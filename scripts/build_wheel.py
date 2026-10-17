"""Package prebuilt Rust/WASM/Jupyter assets without installing into the current environment."""
from pathlib import Path
import argparse
import json
import os
import re
import shutil
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--repair", action="store_true", help="Use auditwheel to produce a manylinux wheel")
parser.add_argument('--windows',action='store_true',help='Package the prebuilt Windows x86_64 executable')
args = parser.parse_args()
if args.windows and args.repair: parser.error('--repair is for Linux wheels')
package = root / "jupyter/fitsviewer"
vendor = package / "_vendor"
(vendor / "bin").mkdir(parents=True, exist_ok=True)
executable = 'fitsviewer.exe' if args.windows else 'fitsviewer'
(vendor/'bin'/('fitsviewer' if args.windows else 'fitsviewer.exe')).unlink(missing_ok=True)
source_binary = root/('target/x86_64-pc-windows-gnu/release/fitsviewer.exe' if args.windows else 'target/release/fitsviewer')
shutil.copy2(source_binary, vendor/'bin'/executable)
if args.windows:
    os.environ['FITSVIEWER_PACKAGE_TARGET']='windows'
    shutil.copy2(root/'windows/app.ico',vendor/'bin/app.ico')
    project=root/'jupyter/pyproject.toml'
    text=project.read_text()
    if 'LicenseRef-MinGW-w64-CRT' not in text:
        text=re.sub(r'^license = "(.*)"$',lambda m:'license = "'+m.group(1)+' AND (LicenseRef-MinGW-w64-CRT) AND (GPL-3.0-or-later WITH GCC-exception-3.1)"',text,flags=re.MULTILINE)
        project.write_text(text)
shutil.copytree(root / "web", vendor / "web", dirs_exist_ok=True)
shutil.copy2(root / "NOTICE.md", vendor / "NOTICE.md")
for name in ["DEPENDENCIES.txt", "requirements.txt"]:
    shutil.copy2(root / name, vendor / name)
if not (root / "THIRD_PARTY_NOTICES.md").is_file():
    subprocess.run([sys.executable, str(root / "scripts/license_notices.py")], cwd=root, check=True)
for name in ["README.md", "README.zh-CN.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]:
    shutil.copy2(root / name, root / "jupyter" / name)
shutil.copytree(root / "licenses", root / "jupyter/licenses", dirs_exist_ok=True)
# Explicit paths avoid shallow glob expansion omitting nested crate/font notices.
# 显式列出许可文件，避免多级目录中的依赖和字体许可漏入 wheel。
license_files = ["LICENSE", "THIRD_PARTY_NOTICES.md"] + [p.relative_to(root / "jupyter").as_posix()
    for p in sorted((root / "jupyter/licenses").rglob("*")) if p.is_file()]
project = root / "jupyter/pyproject.toml"
license_setting = "license-files = [\n" + "".join("  " + json.dumps(p) + ",\n" for p in license_files) + "]"
project.write_text(re.sub(r"license-files\s*=\s*\[[\s\S]*?\]", lambda _: license_setting, project.read_text(), count=1))
install = {"packageManager": "python", "packageName": "fitsviewer",
           "uninstallInstructions": "python -m pip uninstall fitsviewer"}
(package / "labextension/install.json").write_text(json.dumps(install, indent=2))
raw = root / "dist/raw"
raw.mkdir(parents=True, exist_ok=True)
subprocess.run([sys.executable, "-m", "hatchling", "build", "-t", "wheel", "-d", str(raw)],
               cwd=root / "jupyter", check=True)
wheels = sorted(raw.glob("fitsviewer-*.whl"), key=lambda p: p.stat().st_mtime)
wheel = wheels[-1]
if args.repair:
    repair = [sys.executable, "-m", "auditwheel", "repair", str(wheel), "-w", str(root / "dist")]
    if os.environ.get("FITSVIEWER_WHEEL_PLATFORM"): repair += ["--plat", os.environ["FITSVIEWER_WHEEL_PLATFORM"]]
    subprocess.run(repair, check=True)
else:
    shutil.copy2(wheel, root / "dist" / wheel.name)
print("Wheel ready in", root / "dist")
