"""Narrow 0.1.7 UI follow-up: verify reuse scope and newly compiled packages.

Historical runtime acceptance keeps its original source identity. This script
does not execute GUI, Jupyter, scientific, worker or installer-option regressions.
"""
from pathlib import Path, PurePosixPath
import argparse
import hashlib
import json
import os
import re
import subprocess
import tempfile
import zipfile

import build_macos_full as shared
import stage_windows_revalidation as origin

ROOT = Path(__file__).resolve().parents[1]
UI_FILES = {"crates/viewer/src/lib.rs", "crates/viewer/src/ribbon.rs",
            "crates/viewer/src/document_tabs.rs", "crates/viewer/src/multiview.rs"}
CHECKS = {"document_tabs_below_toolbar", "view_measure_toolbar_height_stable",
          "pro_tabs_share_workspace_row"}
PLATFORMS = {"windows-x86_64", "linux-x86_64", "macos-arm64", "macos-x86_64"}
FEATURES = {"base": True, "pro": True, "desktop": True}


def sha(data):
    return hashlib.sha256(data).hexdigest()


def source_sha(data):
    """Source/proof identity is independent of Windows checkout line endings."""
    return sha(data.replace(b"\r\n", b"\n"))


def git(*args):
    return subprocess.check_output(["git", "-C", str(ROOT), *args], text=True).strip()


def blob(source, path):
    return subprocess.check_output(["git", "-C", str(ROOT), "show", source + ":" + path])


def relative(path):
    value = PurePosixPath(path)
    assert not value.is_absolute() and ".." not in value.parts and "\\" not in path and ":" not in path
    return value.as_posix()


def record_check(record, record_path, source):
    assert record["schema"] == 1 and record["kind"] == "ui-only-0.1.7"
    assert record["hash_convention"] == "sha256-lf"
    assert record["baseline_source"] == origin.SOURCE and source != origin.SOURCE
    assert set(record["runtime_sha256"]) == UI_FILES
    for path, expected in record["runtime_sha256"].items():
        assert re.fullmatch(r"[0-9a-f]{64}", expected) and source_sha(blob(source, path)) == expected, path
    changed = set(git("diff", "--name-only", origin.SOURCE, source, "--").splitlines())
    nonruntime = record["nonruntime_changes"]
    assert len(nonruntime) == len(set(nonruntime))
    assert changed & UI_FILES and changed - UI_FILES == set(nonruntime), sorted(changed - UI_FILES)
    for path in nonruntime:
        assert (relative(path).startswith(("scripts/", "qa/", "docs/", ".github/workflows/"))
                or path in {"AgentWorkLog.md", "CHANGELOG.md"}), path
    assert record_path in nonruntime and json.loads(blob(source, record_path)) == record
    target = record["targeted"]
    assert relative(target["path"]).startswith("qa/") and target["path"] in nonruntime
    raw = blob(source, target["path"])
    assert source_sha(raw) == target["sha256"]
    proof = json.loads(raw)
    assert proof["schema"] == 1 and proof["kind"] == "targeted-ui-0.1.7" and proof["passed"] is True
    assert proof["hash_convention"] == "sha256-lf"
    assert proof["source_sha256"] == record["runtime_sha256"]
    assert all(proof["checks"].get(check) is True for check in CHECKS)
    history = record["origin_evidence"]
    assert history["schema"] == 2 and history["source_commit"] == origin.SOURCE
    assert history["run_id"] == origin.ORIGIN_RUN and history["run_attempt"] == 1
    assert history["supplemental_windows"]["run_id"] == 40970505458
    assert history["supplemental_windows"]["driver_commit"] == "d867ad32491e1692c9b8382be8dee94554cc846d"
    assert history["shared_artifact"] == origin.SHARED_NAME
    assert set(history["artifacts"]) == {"full-" + p for p in PLATFORMS} | {origin.SHARED_NAME}
    assert history["artifacts"][origin.SHARED_NAME] == {"id": origin.SHARED_ID, "sha256": origin.SHARED_SHA}
    return proof


def verify_origin_metadata(record):
    history = record["origin_evidence"]
    run = origin.api(f"repos/{origin.REPO}/actions/runs/{origin.ORIGIN_RUN}")
    jobs = origin.api(f"repos/{origin.REPO}/actions/runs/{origin.ORIGIN_RUN}/jobs?filter=all&per_page=100")["jobs"]
    origin.validate_origin(run, jobs)
    extra = history["supplemental_windows"]
    win = origin.api(f"repos/{origin.REPO}/actions/runs/{extra['run_id']}")
    assert win["head_sha"] == extra["driver_commit"] and win["path"] == origin.WORKFLOW
    assert win["event"] == "push" and win["head_branch"] == "codex/windows-revalidate-017"
    assert win["conclusion"] == "success" and win["status"] == "completed" and win["run_attempt"] == extra["run_attempt"]
    jobs = origin.latest_jobs(origin.api(f"repos/{origin.REPO}/actions/runs/{win['id']}/jobs?filter=all&per_page=100")["jobs"])
    assert set(jobs) == {origin.WINDOWS_JOB} and jobs[origin.WINDOWS_JOB]["conclusion"] == "success"
    for name, expected in history["artifacts"].items():
        item = origin.api(f"repos/{origin.REPO}/actions/artifacts/{expected['id']}")
        owner = win if name == "full-windows-x86_64" else run
        assert item["name"] == name and not item["expired"]
        assert item["digest"] == "sha256:" + expected["sha256"]
        assert item["workflow_run"]["id"] == owner["id"] and item["workflow_run"]["head_sha"] == owner["head_sha"]


def archive_bytes(path):
    with zipfile.ZipFile(path) as archive:
        assert archive.testzip() is None
        names = [e.filename for e in archive.infolist() if not e.is_dir()]
        assert len(names) == len(set(n.casefold() for n in names))
        for name in names:
            relative(name)
        return {name: archive.read(name) for name in names}


def ending(files, suffix):
    found = [data for name, data in files.items() if name.endswith(suffix) and not name.startswith("__MACOSX/")]
    assert len(found) == 1, suffix
    return found[0]


def architecture(data, platform):
    if platform == "windows-x86_64":
        assert data[:2] == b"MZ"
        offset = int.from_bytes(data[60:64], "little")
        assert data[offset:offset+4] == b"PE\0\0" and int.from_bytes(data[offset+4:offset+6], "little") == 0x8664
    elif platform == "linux-x86_64":
        assert data[:6] == b"\x7fELF\x02\x01" and int.from_bytes(data[18:20], "little") == 62
    else:
        assert data[:4] == b"\xcf\xfa\xed\xfe"
        assert int.from_bytes(data[4:8], "little") == (0x0100000C if platform.endswith("arm64") else 0x01000007)


def package_check(platform, lock, wheel=None, package_dir=None, windows_dir=None, execute_native=True):
    """Check new archive bytes and run only the matching native --build-info."""
    files, packages = {}, {}
    if wheel:
        files = archive_bytes(wheel)
        packages[wheel.name] = shared.digest(wheel)
        binary = ending(files, "/bin/fitsviewer.exe" if platform.startswith("windows") else "/bin/fitsviewer")
        wasm = ending(files, "/web/pkg/fitsviewer_web_bg.wasm")
    else:
        package = json.loads((package_dir / "package.json").read_text())
        assert package["source_commit"] == lock["source_commit"] and package["architecture"] == platform.split("-", 1)[1]
        path = package_dir / package["portable"]
        assert path.parent == package_dir and shared.digest(path) == package["portable_sha256"]
        packages[path.name] = shared.digest(path)
        files = archive_bytes(path)
        binary = ending(files, "/Contents/Resources/bin/fitsviewer")
        wasm = ending(files, "/Contents/Resources/web/pkg/fitsviewer_web_bg.wasm")
        assert sha(binary) == package["native_binary_sha256"]
        assert sha(wasm) == package["wasm_sha256"]
    assert sha(wasm) == lock["wasm_sha256"]
    architecture(binary, platform)
    if platform.startswith("windows"):
        portable = windows_dir / "FitsViewer-0.1.7-windows-x86_64-portable.zip"
        payload = archive_bytes(portable)
        assert ending(payload, "/fitsviewer.exe") == binary
        assert ending(payload, "/web/pkg/fitsviewer_web_bg.wasm") == wasm
        # Every vendor file must survive the wheel -> portable conversion exactly.
        for name, data in files.items():
            prefix = "fitsviewer/_vendor/"
            if name.startswith(prefix):
                relative_name = name[len(prefix):]
                if relative_name.startswith("bin/"):
                    relative_name = relative_name[4:]
                assert ending(payload, "/" + relative_name) == data, relative_name
        setup = windows_dir / "FitsViewer-0.1.7-windows-x86_64-setup.exe"
        assert setup.stat().st_size > 1024 and setup.read_bytes()[:2] == b"MZ"
        packages.update({p.name: shared.digest(p) for p in [portable, setup]})
    info = None
    if execute_native:
        with tempfile.TemporaryDirectory(prefix="fitsviewer-ui-package-") as temp:
            target = Path(temp) / ("fitsviewer.exe" if platform.startswith("windows") else "fitsviewer")
            target.write_bytes(binary)
            target.chmod(0o755)
            info = json.loads(subprocess.check_output([str(target), "--build-info"], text=True, timeout=30))
            assert info == {"schema": 1, "version": "0.1.7", "features": FEATURES}
            if platform.startswith("linux"):
                versions = subprocess.check_output(["readelf", "--version-info", str(target)], text=True)
                glibc = {tuple(map(int, value.split('.'))) for value in re.findall(r"GLIBC_([\d.]+)", versions)}
                assert glibc and max(glibc) <= (2, 28)
    return {"packages": packages, "native_binary_sha256": sha(binary), "build_info": info,
            "shared_wasm_matches": True, "wasm_sha256": sha(wasm), "architecture_checked": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--record", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--origin-metadata", action="store_true")
    parser.add_argument("--platform", choices=sorted(PLATFORMS))
    parser.add_argument("--assets-lock", type=Path)
    parser.add_argument("--wheel", type=Path)
    parser.add_argument("--package", type=Path)
    parser.add_argument("--windows-dir", type=Path)
    args = parser.parse_args()
    source = git("rev-parse", "HEAD")
    assert os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_SHA") == source
    assert os.environ.get("GITHUB_REPOSITORY") == origin.REPO
    assert not git("status", "--porcelain", "--untracked-files=no"), "Use an unmodified checkout"
    path = args.record.resolve().relative_to(ROOT).as_posix()
    raw = args.record.read_bytes()
    record = json.loads(raw)
    proof = record_check(record, path, source)
    if args.origin_metadata:
        verify_origin_metadata(record)
    report = {"schema": 1, "kind": "ui-only-package-0.1.7" if args.platform else "ui-only-scope-0.1.7",
              "source_commit": source, "run_id": int(os.environ["GITHUB_RUN_ID"]),
              "run_attempt": int(os.environ["GITHUB_RUN_ATTEMPT"]), "ui_only": True,
              "record_path": path, "record_sha256": source_sha(raw), "baseline_source": origin.SOURCE,
              "hash_convention": "sha256-lf",
              "runtime_sha256": record["runtime_sha256"], "targeted": proof,
              "origin_artifact_metadata_verified": args.origin_metadata,
              "runtime_regressions": "reused original evidence; not rerun or relabelled"}
    if args.platform:
        shared.LOCK_PATH = args.assets_lock.resolve()
        lock = shared.lock()
        assert lock["schema"] == 2
        report.update(platform=args.platform, **package_check(args.platform, lock,
            args.wheel.resolve() if args.wheel else None, args.package.resolve() if args.package else None,
            args.windows_dir.resolve() if args.windows_dir else None))
    report["passed"] = True
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    main()
