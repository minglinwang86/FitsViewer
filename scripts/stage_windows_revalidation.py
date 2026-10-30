"""Stage reviewed historical same-source resources for the Windows-only 0.1.7 rerun.

This is a separate, narrowly pinned provenance path. The normal same-run lock()
and its environment checks are never modified or bypassed.
"""
from pathlib import Path, PurePosixPath
import argparse
import hashlib
import json
import os
import re
import shutil
import stat
import subprocess
import sys
import zipfile

REPO = "minglinwang86/FitsViewer"
SOURCE = "f613840afb90fa88f9bc41d0f1c88e35e4bffa10"
ORIGIN_RUN = 40968317884
SHARED_ID = 12938828636
SHARED_NAME = "full-shared-40968317884"
SHARED_SHA = "67a2221319dcf6ae48657028ac87898f2b482948d1fb88936de72602a4056356"
WORKFLOW = ".github/workflows/windows-revalidate-017.yml"
WINDOWS_JOB = "Windows x86_64 Full revalidation"
DRIVER = Path(__file__).resolve().parents[1]
GOOD_JOBS = {"Full shared WASM and Lab resources", "Linux Full package from shared release resources",
             "macOS Full / arm64", "macOS Full / x86_64"}
ORIGIN_WINDOWS = "Windows x86_64 Full"
ORIGIN_NATIVE_STEP = "Build native Full host; check Windows window registry and IPC"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", path], text=True))


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def latest_jobs(history):
    return {j["name"]: j for j in sorted(history, key=lambda j: (j["started_at"] or "", j["id"]))}


def validate_origin(run, history):
    assert run["id"] == ORIGIN_RUN and run["head_sha"] == SOURCE
    assert run["path"] == ".github/workflows/full-release.yml"
    assert run["event"] in {"push", "workflow_dispatch"}
    assert run["status"] == "completed" and run["conclusion"] == "failure"
    jobs = latest_jobs(history)
    assert set(jobs) == GOOD_JOBS | {ORIGIN_WINDOWS}
    assert all(jobs[name]["conclusion"] == "success" for name in GOOD_JOBS)
    assert jobs[ORIGIN_WINDOWS]["conclusion"] == "failure", "Windows must be the only failed job"
    steps = {step["name"]: step for step in jobs[ORIGIN_WINDOWS]["steps"]}
    assert steps[ORIGIN_NATIVE_STEP]["conclusion"] == "success", "Original native build and IPC tests must have passed"


def identity(application, allow_qa=False):
    assert git(application, "rev-parse", "HEAD") == SOURCE
    assert git(DRIVER, "rev-parse", "HEAD") == os.environ["GITHUB_SHA"]
    assert os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REPOSITORY") == REPO
    assert os.environ.get("GITHUB_REF") == "refs/heads/codex/windows-revalidate-017"
    for key in ["GITHUB_RUN_ID", "GITHUB_RUN_ATTEMPT"]:
        assert os.environ[key].isdecimal() and int(os.environ[key]) > 0
    assert int(os.environ["GITHUB_RUN_ID"]) != ORIGIN_RUN
    assert not git(application, "diff", "--cached", "--name-only")
    dirty = git(application, "diff", "--name-only").splitlines()
    assert set(dirty) <= ({"qa/histogram_window.cjs"} if allow_qa else set()), dirty
    if allow_qa:
        assert (application / "qa/histogram_window.cjs").read_bytes() == (DRIVER / "qa/histogram_window.cjs").read_bytes()
    # This complete Git blob inventory binds native inputs to the tested checkout.
    native = git(application, "ls-tree", "-r", SOURCE, "--", "Cargo.toml", "Cargo.lock", ".cargo",
                 "crates", "vendor", "windows")
    assert native
    return {"source_commit": SOURCE, "application_tree": git(application, "rev-parse", "HEAD^{tree}"),
            "native_source_blobs_sha256": hashlib.sha256(native.encode()).hexdigest(),
            "driver_commit": os.environ["GITHUB_SHA"], "driver_tree": git(DRIVER, "rev-parse", "HEAD^{tree}"),
            "run_id": int(os.environ["GITHUB_RUN_ID"]), "run_attempt": int(os.environ["GITHUB_RUN_ATTEMPT"])}


def download(application, resources):
    info = identity(application)
    assert not resources.exists(), "Use a new owned resource directory"
    run = api(f"repos/{REPO}/actions/runs/{ORIGIN_RUN}")
    jobs = api(f"repos/{REPO}/actions/runs/{ORIGIN_RUN}/jobs?filter=all&per_page=100")["jobs"]
    validate_origin(run, jobs)
    artifact = api(f"repos/{REPO}/actions/artifacts/{SHARED_ID}")
    assert artifact["name"] == SHARED_NAME and not artifact["expired"]
    assert artifact["workflow_run"]["id"] == ORIGIN_RUN and artifact["workflow_run"]["head_sha"] == SOURCE
    assert artifact["digest"] == "sha256:" + SHARED_SHA
    resources.mkdir(parents=True)
    archive_path = resources / "origin-shared.zip"
    with archive_path.open("xb") as stream:
        subprocess.run(["gh", "api", f"repos/{REPO}/actions/artifacts/{SHARED_ID}/zip"], stdout=stream, check=True)
    assert sha(archive_path) == SHARED_SHA
    with zipfile.ZipFile(archive_path) as archive:
        assert archive.testzip() is None
        seen = set()
        for entry in archive.infolist():
            name = PurePosixPath(entry.filename)
            assert not name.is_absolute() and ".." not in name.parts and "\\" not in entry.filename and ":" not in entry.filename
            assert entry.filename.casefold() not in seen and not stat.S_ISLNK(entry.external_attr >> 16)
            assert entry.file_size < 256 * 1024 * 1024
            seen.add(entry.filename.casefold())
        archive.extractall(resources)
    lock = json.loads((resources / "assets-lock.json").read_text())
    assert lock["schema"] == 2 and lock["origin"] == "github-actions-same-run"
    assert lock["version"] == "0.1.7"
    assert lock["repository"] == REPO and lock["source_commit"] == SOURCE
    assert int(lock["run_id"]) == ORIGIN_RUN and 0 < int(lock["run_attempt"]) <= run["run_attempt"]
    wheel = resources / lock["wheel_asset"]
    assert wheel.parent == resources and wheel.name.endswith(".whl")
    assert wheel.stat().st_size == lock["wheel_bytes"] and sha(wheel) == lock["wheel_sha256"]
    assert re.fullmatch(r"[0-9a-f]{64}", lock["wasm_sha256"])
    info.update(schema=1, kind="supplemental-windows-0.1.7",
                origin_run_id=ORIGIN_RUN, origin_run_attempt=run["run_attempt"],
                origin_shared_artifact={"id": SHARED_ID, "name": SHARED_NAME, "sha256": SHARED_SHA},
                resources={"wheel_sha256": lock["wheel_sha256"], "wasm_sha256": lock["wasm_sha256"],
                           "assets_lock_sha256": sha(resources / "assets-lock.json")})
    info["reused_windows_ipc"] = {"origin_run_id": ORIGIN_RUN, "step": ORIGIN_NATIVE_STEP,
                                  "conclusion": "success", "source_commit": SOURCE}
    (resources / "provenance.json").write_text(json.dumps(info, indent=2) + "\n")
    (resources / "native-source-blobs.txt").write_text(git(application, "ls-tree", "-r", SOURCE, "--",
        "Cargo.toml", "Cargo.lock", ".cargo", "crates", "vendor", "windows") + "\n")


def stage(application, resources, native, output):
    current = identity(application)
    proof = json.loads((resources / "provenance.json").read_text())
    assert all(proof[k] == v for k, v in current.items())
    assert sha(resources / "origin-shared.zip") == SHARED_SHA
    lock = json.loads((resources / "assets-lock.json").read_text())
    assert sha(resources / "assets-lock.json") == proof["resources"]["assets_lock_sha256"]
    wheel = resources / lock["wheel_asset"]
    assert sha(wheel) == proof["resources"]["wheel_sha256"] and wheel.stat().st_size == lock["wheel_bytes"]
    # Import the pinned application's packaging inventory, not driver runtime files.
    sys.path.insert(0, str(application / "scripts"))
    from build_profiles import select, python_files
    from build_wheel import validate_assets
    assert not output.exists() and output.resolve().is_relative_to((application / "build").resolve())
    inventory = json.loads((resources / "resources-manifest.json").read_text())
    assert inventory["source_commit"] == SOURCE
    metadata = select("full").metadata()
    with zipfile.ZipFile(wheel) as archive:
        seen = set()
        for entry in archive.infolist():
            prefix = next((p for p in ["fitsviewer/_vendor/web/", "fitsviewer/labextension/"]
                           if entry.filename.startswith(p)), None)
            assert not entry.filename.startswith("fitsviewer/_vendor/bin/")
            if not prefix or entry.is_dir():
                continue
            relative = PurePosixPath(entry.filename[len(prefix):])
            assert not relative.is_absolute() and ".." not in relative.parts and "\\" not in str(relative)
            assert not stat.S_ISLNK(entry.external_attr >> 16) and entry.file_size <= 32 * 1024 * 1024
            assert entry.filename not in seen
            seen.add(entry.filename)
            content = archive.read(entry)
            assert inventory["files"][entry.filename] == {"bytes": len(content), "sha256": hashlib.sha256(content).hexdigest()}
            destination = output / ("web" if prefix.endswith("web/") else "labextension") / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(content)
    resource_names = {n for n in inventory["files"] if n.startswith(("fitsviewer/_vendor/web/", "fitsviewer/labextension/"))}
    assert resource_names == seen, "Shared wheel resource inventory differs"
    assert json.loads((output / "web/build-profile.json").read_text()) == metadata
    assert sha(output / "web/pkg/fitsviewer_web_bg.wasm") == lock["wasm_sha256"]
    adapters = {p.name for p in (output / "web/python").glob("*.py")}
    assert adapters == python_files(select("full"))
    for name in adapters:
        assert (output / "web/python" / name).read_bytes().replace(b"\r\n", b"\n") == (
            application / "web/python" / name).read_bytes().replace(b"\r\n", b"\n")
    assert (output / "labextension/package.json").is_file()
    info = json.loads(subprocess.check_output([str(native), "--build-info"], text=True))
    assert info == {"schema": 1, "version": "0.1.7", "features": {"base": True, "pro": True, "desktop": True}}
    (output / "bin").mkdir()
    shutil.copy2(native, output / "bin/fitsviewer.exe")
    (output / "build-profile.json").write_text(json.dumps(metadata, indent=2) + "\n")
    files = {p.relative_to(output).as_posix(): {"bytes": p.stat().st_size, "sha256": sha(p)}
             for p in sorted(output.rglob("*")) if p.is_file()}
    (output / "artifact-manifest.json").write_text(json.dumps({"schema": 1, "features": metadata, "files": files}, indent=2) + "\n")
    validate_assets(output)
    proof.update(native_binary_sha256=sha(native), build_info=info)
    (resources / "provenance.json").write_text(json.dumps(proof, indent=2) + "\n")


def receipt(application, resources):
    current = identity(application, allow_qa=True)
    proof = json.loads((resources / "provenance.json").read_text())
    assert all(proof[k] == v for k, v in current.items())
    qa = (DRIVER / "qa/histogram_window.cjs").read_bytes()
    proof["qa_override"] = {"path": "qa/histogram_window.cjs", "sha256": hashlib.sha256(qa).hexdigest(),
                            "sha256_lf": hashlib.sha256(qa.replace(b"\r\n", b"\n")).hexdigest()}
    proof["runtime_tracked_unchanged"] = True
    wheels = list((application / "dist/full").glob("*win_amd64.whl"));assert len(wheels) == 1
    with zipfile.ZipFile(wheels[0]) as archive:
        assert hashlib.sha256(archive.read("fitsviewer/_vendor/web/pkg/fitsviewer_web_bg.wasm")).hexdigest() == proof["resources"]["wasm_sha256"]
        assert hashlib.sha256(archive.read("fitsviewer/_vendor/bin/fitsviewer.exe")).hexdigest() == proof["native_binary_sha256"]
    packages = [*wheels, *(application / "dist/windows").glob("*.exe"), *(application / "dist/windows").glob("*.zip")]
    assert len(packages) == 3
    proof["packages"] = {p.name: sha(p) for p in packages}
    (resources / "provenance.json").write_text(json.dumps(proof, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["download", "stage", "receipt"])
    parser.add_argument("--application", type=Path, required=True)
    parser.add_argument("--resources", type=Path, required=True)
    parser.add_argument("--native", type=Path)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    application, resources = args.application.resolve(), args.resources.resolve()
    if args.action == "download":
        download(application, resources)
    elif args.action == "stage":
        assert os.name == "nt" and args.native and args.output
        stage(application, resources, args.native.resolve(), args.output.resolve())
    else:
        receipt(application, resources)

