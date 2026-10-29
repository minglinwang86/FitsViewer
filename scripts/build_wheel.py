"""Package selected, prebuilt assets in isolation; never rewrite application source.

仅打包已构建的模块产物，在独立目录组装 wheel，不修改业务源码。
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys

from build_profiles import ROOT, python_files, registry, select


def validate_assets(directory):
    """Check the selected profile and every staged byte before packaging."""
    directory = directory.resolve()
    metadata = json.loads((directory / "build-profile.json").read_text(encoding="utf-8"))
    selection = select(modules=metadata["modules"])
    expected = (select(metadata["profile"]) if metadata["profile"] in registry()["profiles"] else selection).metadata()
    if metadata != expected:
        raise ValueError("Invalid staged build profile")
    manifest = json.loads((directory / "artifact-manifest.json").read_text(encoding="utf-8"))
    if manifest.get("schema") != 1 or manifest.get("features") != metadata:
        raise ValueError("Asset manifest does not match the build profile")
    actual = {p.relative_to(directory).as_posix() for p in directory.rglob("*") if p.is_file()}
    if actual - {"artifact-manifest.json"} != set(manifest["files"]):
        raise ValueError("Staged asset inventory has changed")
    for name, recorded in manifest["files"].items():
        path = directory / name
        if not path.resolve().is_relative_to(directory) or path.is_symlink():
            raise ValueError("Asset path escaped its stage")
        if path.stat().st_size != recorded["bytes"] or hashlib.sha256(path.read_bytes()).hexdigest() != recorded["sha256"]:
            raise ValueError("Staged asset changed: " + name)
    if json.loads((directory / "web/build-profile.json").read_text(encoding="utf-8")) != metadata:
        raise ValueError("Web and package profiles differ")
    adapters = {path.name for path in (directory / "web/python").glob("*.py")}
    if adapters != python_files(selection):
        raise ValueError("Staged Python adapters differ from the selected modules")
    return metadata


def remove_table(text, name):
    return re.sub(r"(?ms)^\[" + re.escape(name) + r"\][^\n]*\n.*?(?=^\[|\Z)", "", text)


def prepare(assets=None, windows=False):
    """Assemble an owned packaging directory. Does not compile or invoke Hatch."""
    metadata = validate_assets(assets) if assets else select("full").metadata()
    selected = select(modules=metadata["modules"])
    parent = (ROOT / "build/python-packages").resolve()
    if not parent.is_relative_to(ROOT.resolve()):
        raise ValueError("Packaging output escaped the project")
    output = parent / (selected.name + ("-windows" if windows else "-linux"))
    if output.is_symlink() or output.resolve().parent != parent:
        raise ValueError("Unsafe packaging directory")
    marker = output / ".fitsviewer-packaging-stage"
    if output.exists():
        if not marker.is_file():
            raise ValueError("Refusing to replace an unrecognized packaging directory")
        shutil.rmtree(output)
    output.mkdir(parents=True)
    marker.write_text("schema=1\n", encoding="utf-8")
    omitted = {"_vendor", "labextension", "__pycache__", "*.pyc", "*.pyo"}
    if not metadata["pro"]:
        omitted.add("samp_bridge.py")
    if not metadata["jupyter"]:
        omitted.add("server.py")
    shutil.copytree(ROOT / "jupyter/fitsviewer", output / "fitsviewer", ignore=shutil.ignore_patterns(*omitted))
    package = output / "fitsviewer"
    vendor = package / "_vendor"
    if assets:
        shutil.copytree(assets / "bin", vendor / "bin")
        shutil.copytree(assets / "web", vendor / "web")
        shutil.copy2(assets / "artifact-manifest.json", vendor / "artifact-manifest.json")
    else:
        # Legacy explicit full packaging uses the traditional default-full build paths.
        binary = ROOT / ("target/x86_64-pc-windows-gnu/release/fitsviewer.exe" if windows else "target/release/fitsviewer")
        (vendor / "bin").mkdir(parents=True)
        shutil.copy2(binary, vendor / "bin" / ("fitsviewer.exe" if windows else "fitsviewer"))
        shutil.copytree(ROOT / "web", vendor / "web", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "*.pyo"))
    if windows:
        shutil.copy2(ROOT / "windows/app.ico", vendor / "bin/app.ico")
    for path in [package / "build-profile.json", vendor / "web/build-profile.json"]:
        path.write_text(json.dumps(metadata, indent=2) + "\n", encoding="utf-8")
    for name in ["NOTICE.md", "DEPENDENCIES.txt", "requirements.txt", "requirements-spectra.txt"]:
        shutil.copy2(ROOT / name, vendor / name)
    for name in ["README.md", "README.zh-CN.md", "LICENSE", "THIRD_PARTY_NOTICES.md"]:
        shutil.copy2(ROOT / name, output / name)
    shutil.copytree(ROOT / "licenses", output / "licenses")
    shutil.copy2(ROOT / "jupyter/hatch_build.py", output / "hatch_build.py")
    if metadata["jupyter"]:
        # Cross-platform stages may carry the same manifest-verified Lab build.
        lab_source = assets / "labextension" if assets and (assets / "labextension").is_dir() else ROOT / "jupyter/fitsviewer/labextension"
        shutil.copytree(lab_source, package / "labextension")
        shutil.copytree(ROOT / "jupyter/jupyter-config", output / "jupyter-config")
        (package / "labextension/install.json").write_text(json.dumps({"packageManager": "python", "packageName": "fitsviewer", "uninstallInstructions": "python -m pip uninstall fitsviewer"}), encoding="utf-8")
    text = (ROOT / "jupyter/pyproject.toml").read_text(encoding="utf-8")
    if not metadata["jupyter"]:
        for name in ["project.entry-points.jupyter_serverproxy_servers", "tool.hatch.build.targets.wheel.shared-data"]:
            text = remove_table(text, name)
    license_files = ["LICENSE", "THIRD_PARTY_NOTICES.md"] + [p.relative_to(output).as_posix() for p in sorted((output / "licenses").rglob("*")) if p.is_file()]
    setting = "license-files = [\n" + "".join("  " + json.dumps(p) + ",\n" for p in license_files) + "]"
    text = re.sub(r"license-files\s*=\s*\[[\s\S]*?\]", lambda _: setting, text, count=1)
    (output / "pyproject.toml").write_text(text, encoding="utf-8")
    return output, metadata


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--assets-dir", type=Path, help="Profile assets produced by build_profiles.py --execute")
    parser.add_argument("--repair", action="store_true", help="Use auditwheel for the actual Linux ABI")
    parser.add_argument("--windows", action="store_true", help="Package an explicitly prebuilt Windows x86_64 executable")
    args = parser.parse_args(argv)
    if args.windows and args.repair:
        parser.error("--repair is for Linux wheels")
    if not (ROOT / "THIRD_PARTY_NOTICES.md").is_file():
        raise FileNotFoundError("Generate the third-party notices before packaging")
    stage, metadata = prepare(args.assets_dir, args.windows)
    destination = ROOT / "dist" / metadata["profile"]
    raw = destination / "raw"
    raw.mkdir(parents=True, exist_ok=True)
    environment = os.environ.copy()
    environment["FITSVIEWER_PACKAGE_TARGET"] = "windows" if args.windows else "linux"
    before = {p.name: p.stat().st_mtime_ns for p in raw.glob("fitsviewer-*.whl")}
    subprocess.run([sys.executable, "-m", "hatchling", "build", "-t", "wheel", "-d", str(raw)], cwd=stage, env=environment, check=True)
    wheels = [p for p in raw.glob("fitsviewer-*.whl") if before.get(p.name) != p.stat().st_mtime_ns]
    if len(wheels) != 1:
        raise RuntimeError("Expected exactly one newly generated wheel")
    wheel = wheels[0]
    if args.repair:
        command = [sys.executable, "-m", "auditwheel", "repair", str(wheel), "-w", str(destination)]
        if environment.get("FITSVIEWER_WHEEL_PLATFORM"):
            command += ["--plat", environment["FITSVIEWER_WHEEL_PLATFORM"]]
        subprocess.run(command, check=True, env=environment)
    else:
        shutil.copy2(wheel, destination / wheel.name)
    for path in destination.glob("fitsviewer-*.whl"):
        if path.stat().st_mtime_ns >= wheel.stat().st_mtime_ns:
            shutil.copy2(path, ROOT / "dist" / path.name)
    print("Wheel ready in", destination)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
