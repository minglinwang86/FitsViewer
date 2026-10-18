"""Build a native macOS Full bundle from this checkout and locked shared assets.

仅原生 macOS 构建；复用经 SHA256 锁定的 Full 资源，不复制 Linux 可执行文件。
"""
from pathlib import Path, PurePosixPath
import argparse
import hashlib
import json
import os
import platform
import plistlib
import re
import shutil
import stat
import subprocess
import sys
import tomllib
import zipfile

from build_profiles import ROOT, python_files, select

LOCK_PATH = ROOT / "config/macos-full-assets.json"


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def lock():
    value = json.loads(LOCK_PATH.read_text(encoding="utf-8"))
    if value.get("schema") != 1 or value["repository"] != "minglinwang86/FitsViewer":
        raise ValueError("Unexpected shared-assets lock")
    if Path(value["wheel_asset"]).name != value["wheel_asset"]:
        raise ValueError("Invalid asset name")
    for name in ["wheel_sha256", "wasm_sha256"]:
        if not re.fullmatch(r"[0-9a-f]{64}", value[name]):
            raise ValueError("Invalid locked SHA256")
    return value


def validate_wheel(path, expected):
    if path.stat().st_size != expected["wheel_bytes"] or digest(path) != expected["wheel_sha256"]:
        raise ValueError("Shared Full wheel differs from the reviewed lock")


def extract_resources(wheel, destination):
    """Copy only platform-independent web and Lab files, never _vendor/bin.
    只提取平台无关文件；绝不提取 Linux binary 或任何 Python 解释器。
    """
    expected = lock()
    validate_wheel(wheel, expected)
    prefixes = {"fitsviewer/_vendor/web/": destination / "web",
                "fitsviewer/labextension/": destination / "jupyter/labextension"}
    seen = set()
    with zipfile.ZipFile(wheel) as archive:
        for member in archive.infolist():
            prefix = next((p for p in prefixes if member.filename.startswith(p)), None)
            if prefix is None or member.is_dir():
                continue
            relative = PurePosixPath(member.filename[len(prefix):])
            if relative.is_absolute() or ".." in relative.parts or "\\" in str(relative):
                raise ValueError("Unsafe shared-resource archive path")
            if stat.S_ISLNK(member.external_attr >> 16) or member.file_size > 32 * 1024 * 1024:
                raise ValueError("Unsupported shared-resource entry")
            output = prefixes[prefix].joinpath(*relative.parts)
            if output in seen or not output.resolve().is_relative_to(destination.resolve()):
                raise ValueError("Duplicate or escaping shared-resource path")
            seen.add(output)
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(archive.read(member))
    profile = select("full").metadata()
    if json.loads((destination / "web/build-profile.json").read_text()) != profile:
        raise ValueError("Locked web resources are not Full")
    wasm = destination / "web/pkg/fitsviewer_web_bg.wasm"
    if digest(wasm) != expected["wasm_sha256"]:
        raise ValueError("Locked Full WASM hash differs")
    adapters = {p.name for p in (destination / "web/python").glob("*.py")}
    if adapters != python_files(select("full")):
        raise ValueError("Full scientific adapter inventory differs")
    for name in adapters:
        if digest(destination / "web/python" / name) != digest(ROOT / "web/python" / name):
            raise ValueError("Shared scientific adapter is stale: " + name)
    if not (destination / "jupyter/labextension/package.json").is_file():
        raise ValueError("Prebuilt JupyterLab resources are missing")
    for name in ["index.html", "fonts/cjk.ttf", "icons/fitsviewer.icns"]:
        if not (destination / "web" / name).is_file():
            raise ValueError("Missing Full resource: " + name)
    (destination / "build-profile.json").write_text(json.dumps(profile, indent=2) + "\n")


def command(arguments, environment=None):
    print("+ " + " ".join(map(str, arguments)), flush=True)
    subprocess.run(list(map(str, arguments)), cwd=ROOT, env=environment, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch-assets", type=Path, help="Download only the locked release asset using gh")
    parser.add_argument("--wheel", type=Path, help="Reviewed Linux Full wheel; only shared resources are used")
    parser.add_argument("--output", type=Path, default=ROOT / "dist/macos")
    parser.add_argument("--execute", action="store_true", help="Explicitly compile and package on macOS")
    args = parser.parse_args()
    expected = lock()
    if args.fetch_assets:
        if args.wheel or args.execute:
            parser.error("Asset fetching is a separate operation")
        target = args.fetch_assets.resolve()
        if target.exists():
            raise ValueError("Refusing to replace an existing asset download")
        target.parent.mkdir(parents=True, exist_ok=True)
        command(["gh", "release", "download", expected["release_tag"], "--repo", expected["repository"],
                 "--pattern", expected["wheel_asset"], "--output", target])
        validate_wheel(target, expected)
        print(json.dumps({"assets_verified": True, "sha256": expected["wheel_sha256"]}))
        return
    if not args.execute:
        print(json.dumps({"profile": "full", "features": ["desktop", "pro"], "will_execute": False,
                          "assets": expected, "required_host": "macOS arm64 or x86_64"}, indent=2))
        return
    architecture = {"arm64": "arm64", "aarch64": "arm64", "x86_64": "x86_64"}.get(platform.machine())
    if sys.platform != "darwin" or architecture is None:
        raise RuntimeError("Execute on a native macOS arm64/x86_64 runner")
    if args.wheel is None:
        parser.error("--execute requires the locked --wheel")
    wheel = args.wheel.resolve()
    validate_wheel(wheel, expected)
    output = args.output.resolve()
    if not output.is_relative_to(ROOT.resolve()) or output == ROOT.resolve() or output.exists():
        raise ValueError("Use a new package output directory inside this checkout")
    output.mkdir(parents=True)
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    if "v" + version != expected["release_tag"]:
        raise ValueError("Source version differs from the shared-resource release")
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    build = {"schema": 1, "source_commit": revision, "version": version, "architecture": architecture,
             "os": platform.platform(), "deployment_target": "11.0", "assets": expected,
             "gui_interaction_verified": False, "notarized": False}
    (output / "build.json").write_text(json.dumps(build, indent=2) + "\n")
    environment = os.environ.copy()
    environment["MACOSX_DEPLOYMENT_TARGET"] = "11.0"
    cargo_home = Path(environment.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
    flags = ["--remap-path-prefix=" + str(ROOT) + "=fitsviewer-source",
             "--remap-path-prefix=" + str(cargo_home) + "=cargo-source"]
    if environment.get("RUSTFLAGS") or environment.get("CARGO_ENCODED_RUSTFLAGS"):
        raise ValueError("Use an unmodified RUSTFLAGS environment for this release build")
    environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join(flags)
    environment["CARGO_TARGET_DIR"] = str(ROOT / "target/macos-full")
    command(["cargo", "test", "--locked", "--release", "-p", "fitsviewer-core"], environment)
    command(["cargo", "build", "--locked", "--release", "-p", "fitsviewer",
             "--no-default-features", "--features", "desktop,pro"], environment)
    package_version = version.replace("-rc.", "rc")
    payload = output / ("FitsViewer-" + package_version + "-macos-" + architecture)
    app = payload / "FitsViewer.app"
    resources = app / "Contents/Resources"
    resources.mkdir(parents=True)
    extract_resources(wheel, resources)
    (resources / "bin").mkdir()
    binary = resources / "bin/fitsviewer"
    shutil.copy2(ROOT / "target/macos-full/release/fitsviewer", binary)
    binary.chmod(0o755)
    cpu = {"arm64": 0x0100000C, "x86_64": 0x01000007}[architecture]
    header = binary.read_bytes()[:8]
    if header[:4] != b"\xcf\xfa\xed\xfe" or int.from_bytes(header[4:8], "little") != cpu:
        raise ValueError("The native executable is not a matching 64-bit Mach-O")
    features = json.loads(subprocess.check_output([binary, "--build-info"], text=True))
    if features["version"] != version or features["features"] != {"base": True, "pro": True, "desktop": True}:
        raise ValueError("Native host is not this Full version")
    for name in ["LICENSE", "NOTICE.md", "DEPENDENCIES.txt", "THIRD_PARTY_NOTICES.md",
                 "requirements.txt", "requirements-spectra.txt"]:
        shutil.copy2(ROOT / name, resources / name)
    shutil.copytree(ROOT / "licenses", resources / "licenses")
    shutil.copy2(resources / "web/icons/fitsviewer.icns", resources / "FitsViewer.icns")
    (app / "Contents/MacOS").mkdir()
    launcher = app / "Contents/MacOS/FitsViewer"
    command(["xcrun", "clang", "-Os", "-Wall", "-Wextra", "-Werror", "-arch", architecture,
             "-mmacosx-version-min=11.0", ROOT / "macos/launcher.c", "-o", launcher], environment)
    info = {"CFBundleExecutable": "FitsViewer", "CFBundleIdentifier": "org.fitsviewer.viewer",
            "CFBundleName": "FitsViewer", "CFBundleDisplayName": "FitsViewer",
            "CFBundlePackageType": "APPL", "CFBundleIconFile": "FitsViewer.icns",
            "CFBundleShortVersionString": version.split("-")[0],
            "CFBundleVersion": version.split("-")[0] + "b" + version.rsplit(".", 1)[-1],
            "LSMinimumSystemVersion": "11.0", "NSHighResolutionCapable": True,
            "NSHumanReadableCopyright": "FitsViewer contributors"}
    with (app / "Contents/Info.plist").open("wb") as stream:
        plistlib.dump(info, stream, sort_keys=True)
    for executable in [binary, launcher]:
        command(["codesign", "--force", "--sign", "-", "--timestamp=none", executable])
    command(["codesign", "--force", "--sign", "-", "--timestamp=none", app])
    command(["codesign", "--verify", "--deep", "--strict", "--verbose=2", app])
    wrapper = payload / "fitsviewer"
    wrapper.write_text('#!/bin/sh\nset -eu\nentry=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexec "$entry/FitsViewer.app/Contents/MacOS/FitsViewer" "$@"\n')
    wrapper.chmod(0o755)
    (payload / "README.txt").write_text(
        "FitsViewer " + version + " Full / macOS " + architecture + "\n\n"
        "Open FitsViewer.app, or copy it to ~/Applications and open it.\n"
        "Use Open in the app to select an image or spectrum.\n"
        "Terminal: ./fitsviewer /path/to/image.fits\n"
        "Optional science: ./fitsviewer image.fits --python /path/to/environment/bin/python\n"
        "Python, scientific packages and TeX remain external; they are not bundled.\n"
        "Prebuilt Lab resources are included; this portable app does not install Jupyter extensions.\n"
        "This candidate uses an ad-hoc signature; it has no Developer ID or Apple notarization.\n"
        "macOS 15 is the CI test environment. Older systems have not been tested.\n"
        "Finder file associations and interactive GUI/scientific workflows have not been validated.\n"
        "Scientific results are for reference; verify them independently before use.\n", encoding="utf-8")
    archive = output / (payload.name + "-portable.zip")
    command(["ditto", "-c", "-k", "--keepParent", "--sequesterRsrc", payload, archive])
    archive_hash = digest(archive)
    (output / (archive.name + ".sha256")).write_text(archive_hash + "  " + archive.name + "\n")
    package = {**build, "features": features["features"], "app": str(app.relative_to(output)),
               "portable": archive.name, "portable_sha256": archive_hash, "portable_bytes": archive.stat().st_size,
               "native_binary_sha256": digest(binary), "native_launcher_sha256": digest(launcher),
               "wasm_sha256": digest(resources / "web/pkg/fitsviewer_web_bg.wasm"),
               "signature": "ad-hoc; no Developer ID or notarization"}
    (output / "package.json").write_text(json.dumps(package, indent=2) + "\n")
    print(json.dumps({"packaged": True, "portable": archive.name, "sha256": archive_hash}))


if __name__ == "__main__":
    main()
