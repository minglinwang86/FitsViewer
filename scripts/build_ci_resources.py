"""Build Full shared resources on the exact CI commit, without a native host.

The resource-only wheel is an internal Actions transport. It is not a release
or an installable FitsViewer application, and contains no executable/runtime.
"""
from pathlib import Path
import argparse
import base64
import csv
import hashlib
import io
import json
import os
import re
import shutil
import subprocess
import tomllib
import zipfile

from build_profiles import ROOT, cargo_arguments, python_files, run, select


def digest(data):
    return hashlib.sha256(data).hexdigest()


def build(output):
    source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    if (os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("GITHUB_REPOSITORY") != "minglinwang86/FitsViewer"
            or os.environ.get("GITHUB_SHA") != source
            or not re.fullmatch(r"[0-9]+", os.environ.get("GITHUB_RUN_ID", ""))
            or not re.fullmatch(r"[0-9]+", os.environ.get("GITHUB_RUN_ATTEMPT", ""))):
        raise RuntimeError("Shared CI resources require the exact GitHub Actions checkout identity")
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=ROOT, text=True).strip():
        raise RuntimeError("Build shared CI resources from an unmodified tracked checkout")
    output = output.resolve()
    if output.exists():
        raise ValueError("Use a new output directory")
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    selection = select("full")
    allowed = python_files(selection)
    bindgen_version = next(p["version"] for p in tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
                           if p["name"] == "wasm-bindgen")
    environment = os.environ.copy()
    cargo_home = Path(environment.get("CARGO_HOME", Path.home() / ".cargo")).resolve()
    if environment.get("RUSTFLAGS") or environment.get("CARGO_ENCODED_RUSTFLAGS"):
        raise ValueError("Use unmodified Rust flags for CI resource generation")
    environment["CARGO_ENCODED_RUSTFLAGS"] = "\x1f".join([
        "--remap-path-prefix=" + str(ROOT) + "=fitsviewer-source",
        "--remap-path-prefix=" + str(cargo_home) + "=cargo-source"])
    bindgen = shutil.which("wasm-bindgen")
    actual = subprocess.check_output([bindgen, "--version"], text=True).strip() if bindgen else ""
    if actual != "wasm-bindgen " + bindgen_version:
        run(["cargo", "install", "wasm-bindgen-cli", "--version", bindgen_version, "--locked"], env=environment)
    run(cargo_arguments("fitsviewer-web", selection.viewer_features) + ["--target", "wasm32-unknown-unknown"], env=environment)
    run(["npm", "ci"], ROOT / "jupyter", environment)
    run(["npm", "run", "build"], ROOT / "jupyter", environment)
    if subprocess.check_output(["git", "status", "--porcelain", "--untracked-files=no"], cwd=ROOT, text=True).strip():
        raise RuntimeError("A build command changed tracked source; refusing a same-commit resource lock")
    output.mkdir(parents=True)
    stage = output / "stage"
    web = stage / "fitsviewer/_vendor/web"

    def ignore(directory, names):
        omitted = {n for n in names if n in {"pkg", "__pycache__", "node_modules"} or n.endswith((".pyc", ".pyo"))}
        if Path(directory).resolve() == (ROOT / "web/python").resolve():
            omitted.update(n for n in names if n.endswith(".py") and n not in allowed)
        return omitted

    shutil.copytree(ROOT / "web", web, ignore=ignore)
    target = Path(environment.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    run(["wasm-bindgen", "--target", "web", "--no-typescript", "--out-dir", str(web / "pkg"),
         str(target / "wasm32-unknown-unknown/release/fitsviewer_web.wasm")], env=environment)
    (web / "build-profile.json").write_text(json.dumps(selection.metadata(), indent=2) + "\n", encoding="utf-8")
    shutil.copytree(ROOT / "jupyter/fitsviewer/labextension", stage / "fitsviewer/labextension",
                    ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "*.pyo"))
    dist_name = "fitsviewer_shared_resources-" + version + ".dist-info"
    info = stage / dist_name
    info.mkdir()
    (info / "METADATA").write_text("Metadata-Version: 2.4\nName: fitsviewer-shared-resources\nVersion: " + version
                                  + "\nSummary: Internal same-run CI resources, not a FitsViewer application\n", encoding="utf-8")
    (info / "WHEEL").write_text("Wheel-Version: 1.0\nGenerator: FitsViewer CI\nRoot-Is-Purelib: true\nTag: py3-none-any\n", encoding="utf-8")
    files = sorted(p for p in stage.rglob("*") if p.is_file())
    inventory = {p.relative_to(stage).as_posix(): {"bytes": p.stat().st_size, "sha256": digest(p.read_bytes())} for p in files}
    record = io.StringIO(newline="")
    writer = csv.writer(record, lineterminator="\n")
    for p in files:
        data = p.read_bytes()
        writer.writerow([p.relative_to(stage).as_posix(), "sha256=" + base64.urlsafe_b64encode(hashlib.sha256(data).digest()).decode().rstrip("="), len(data)])
    writer.writerow([dist_name + "/RECORD", "", ""])
    (info / "RECORD").write_text(record.getvalue(), encoding="utf-8", newline="")
    wheel = output / ("fitsviewer_shared_resources-" + version + "-py3-none-any.whl")
    with zipfile.ZipFile(wheel, "x", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        for p in sorted(p for p in stage.rglob("*") if p.is_file()):
            archive.write(p, p.relative_to(stage).as_posix())
    lock = {"schema": 2, "origin": "github-actions-same-run", "repository": os.environ["GITHUB_REPOSITORY"],
            "source_commit": source, "run_id": os.environ["GITHUB_RUN_ID"], "run_attempt": os.environ["GITHUB_RUN_ATTEMPT"],
            "version": version, "wheel_asset": wheel.name, "wheel_bytes": wheel.stat().st_size,
            "wheel_sha256": digest(wheel.read_bytes()), "wasm_sha256": digest((web / "pkg/fitsviewer_web_bg.wasm").read_bytes())}
    (output / "assets-lock.json").write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
    (output / "resources-manifest.json").write_text(json.dumps({"source_commit": source, "files": inventory}, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(lock, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "dist/ci-resources")
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    if args.execute:
        build(args.output)
    else:
        print(json.dumps({"will_execute": False, "profile": "full", "native_host": False, "output": str(args.output)}))
