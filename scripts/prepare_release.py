"""Prepare reviewed GitHub assets without publishing. / 整理发行物，不执行发布。"""
from pathlib import Path
import argparse
import hashlib
import json
import re
import tarfile
import tomllib
import zipfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--wheel", type=Path)
args = parser.parse_args()
project = tomllib.loads((root / "jupyter/pyproject.toml").read_text())["project"]
version = project["version"]
tag = "v" + tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["version"]
candidates = list((root / "dist").glob("fitsview_jupyter-" + version + "-*-manylinux*.whl"))
wheel = args.wheel.resolve() if args.wheel else max(candidates, key=lambda p: p.stat().st_mtime)
with zipfile.ZipFile(wheel) as archive:
    names = archive.namelist()
    assert len(names) == len(set(names)) and archive.testzip() is None
    assert any(".dist-info/licenses/LICENSE" in p for p in names), "Primary license missing"
    for font_notice in ["OFL.txt", "UFL.txt", "Hack-Regular.txt", "emoji-icon-font-mit-license.txt"]:
        assert any(p.endswith("/" + font_notice) for p in names), "Font notice missing: " + font_notice
files = []
skip = {"node_modules", "lib", "__pycache__", "_vendor", "labextension", ".ipynb_checkpoints", "pkg"}
for directory in ["crates", "jupyter", "web", "qa", "docs", "examples", "licenses", ".github", "bin"]:
    for path in (root / directory).rglob("*"):
        rel = path.relative_to(root)
        if path.is_file() and not path.is_symlink() and not skip.intersection(rel.parts):
            if rel.parts[:2] == ("jupyter", "licenses"): continue
            if rel.as_posix().startswith("jupyter/") and len(rel.parts)==2 and rel.name in {"README.md","README.zh-CN.md","LICENSE","THIRD_PARTY_NOTICES.md"}: continue
            files.append(path)
scripts = ["bootstrap.sh", "env.sh", "build.sh", "build_wheel.py", "license_notices.py", "prepare_release.py", "install.py",
           "verify_wheel.py", "verify_wheel.cjs", "verify_memory.py", "verify_memory.cjs", "verify_memory_light.cjs", "verify_privacy.py", "check_management.py", "check_desktop.py", "check_interactive_install.py", "verify_native.py", "verify_glibc.py", "verify_numerics.py", "make_fixtures.py"]
files.extend(root / "scripts" / name for name in scripts)
files.extend(root / name for name in ["Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "requirements-build.txt", "requirements.txt", "DEPENDENCIES.txt",
             ".gitignore", "README.md", "README.zh-CN.md", "LICENSE", "NOTICE.md", "THIRD_PARTY_NOTICES.md",
             "CHANGELOG.md", "CONTRIBUTING.md", "SECURITY.md"])
files = sorted(set(files))
for path in files:
    assert path.is_file(), "Missing release source: " + str(path)
    if path.suffix in {".md", ".json", ".ipynb", ".py", ".sh", ".toml", ".yml"}:
        text = path.read_text()
        assert not re.search(r"token=[A-Za-z0-9]{16,}", text), "Token in " + str(path)
out = root / "release" / version
out.mkdir(parents=True, exist_ok=True)
source = out / ("fitsview-jupyter-" + tag[1:] + "-source.tar.gz")
with tarfile.open(source, "w:gz") as archive:
    def public_metadata(info):
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.pax_headers = {key: value for key, value in info.pax_headers.items() if key not in {"atime", "ctime"}}
        return info
    for path in files: archive.add(path, arcname="fitsview-jupyter-" + tag[1:] + "/" + path.relative_to(root).as_posix(), recursive=False, filter=public_metadata)
wheel_target = out / wheel.name
wheel_target.write_bytes(wheel.read_bytes())
assets = [source, wheel_target]
checksums = out / "SHA256SUMS"
checksums.write_text("".join(hashlib.sha256(p.read_bytes()).hexdigest() + "  " + p.name + "\n" for p in assets))
manifest = {"version": version, "tag": tag, "published": False,
            "source_files": len(files), "assets": {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in [*assets, checksums]}}
(root / "reports").mkdir(exist_ok=True)
(root / "reports/prerelease_assets.json").write_text(json.dumps(manifest, indent=2) + "\n")
print(json.dumps(manifest, indent=2))
