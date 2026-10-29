"""Combine a native Full host with this Actions run's shared web/Lab resources.

This only stages existing files. It does not compile, install or run the viewer.
平台产物复用同一次 CI 的已校验资源，不重复构建 WASM，也不修改科学源码。
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import shutil
import subprocess

import build_macos_full as shared
from build_profiles import ROOT, select
from build_wheel import validate_assets


def stage(native, wheel, lock_path, output, windows):
    shared.LOCK_PATH = lock_path.resolve()
    expected = shared.lock()
    if expected["schema"] != 2:
        raise ValueError("Platform CI staging requires a same-run resource lock")
    output = output.resolve()
    if output.exists() or not output.is_relative_to((ROOT / "build").resolve()):
        raise ValueError("Use a new owned platform stage under build/")
    shared.extract_resources(wheel.resolve(), output)
    shutil.move(str(output / "jupyter/labextension"), output / "labextension")
    (output / "jupyter").rmdir()
    (output / "bin").mkdir()
    name = "fitsviewer.exe" if windows else "fitsviewer"
    shutil.copy2(native.resolve(), output / "bin" / name)
    if not windows:
        (output / "bin" / name).chmod(0o755)
    info = json.loads(subprocess.check_output([str(native.resolve()), "--build-info"], text=True))
    if info["version"] != expected["version"] or info["features"] != {"base": True, "pro": True, "desktop": True}:
        raise ValueError("Native host is not this Full version")
    metadata = select("full").metadata()
    files = {p.relative_to(output).as_posix(): {"bytes": p.stat().st_size,
             "sha256": hashlib.sha256(p.read_bytes()).hexdigest()}
             for p in sorted(output.rglob("*")) if p.is_file()}
    (output / "artifact-manifest.json").write_text(json.dumps(
        {"schema": 1, "features": metadata, "files": files}, indent=2) + "\n", encoding="utf-8")
    validate_assets(output)
    print(json.dumps({"staged": True, "source_commit": expected["source_commit"],
                      "platform": "windows" if windows else "linux", "files": len(files),
                      "wasm_sha256": expected["wasm_sha256"]}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--wheel", required=True, type=Path)
    parser.add_argument("--assets-lock", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--windows", action="store_true")
    args = parser.parse_args()
    if args.windows != (os.name == "nt"):
        parser.error("Stage on the native target runner")
    stage(args.native, args.wheel, args.assets_lock, args.output, args.windows)
