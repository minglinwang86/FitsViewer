"""Publish only explicitly reviewed, successful platform CI artifacts for 0.1.7.

No rebuilding or test rerun occurs here. The release tag points to the tested
source commit, not to the later approval/infrastructure commit.
"""
from pathlib import Path, PurePosixPath
import argparse
import gzip
import hashlib
import json
import os
import re
import subprocess
import zipfile

REPO = "minglinwang86/FitsViewer"
ARTIFACTS = {"full-windows-x86_64", "full-macos-arm64", "full-macos-x86_64", "full-linux-x86_64"}
PACKAGES = {
    "FitsViewer-0.1.7-windows-x86_64-setup.exe",
    "FitsViewer-0.1.7-windows-x86_64-portable.zip",
    "fitsviewer-0.1.7-py3-none-win_amd64.whl",
    "fitsviewer-0.1.7-py3-none-manylinux_2_28_x86_64.whl",
    "FitsViewer-0.1.7-macos-arm64-portable.zip",
    "FitsViewer-0.1.7-macos-x86_64-portable.zip",
}


def command(args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, **kwargs)


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", path], text=True))


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def one(root, filename):
    found = list(root.rglob(filename))
    if len(found) != 1:
        raise ValueError(f"Expected one {filename} in {root.name}, found {len(found)}")
    return found[0]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--approval", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    approval = json.loads(args.approval.read_text())
    if not args.execute:
        print(json.dumps({"will_publish": False, "approval": approval}))
        return
    assert os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REPOSITORY") == REPO
    assert approval["schema"] == 1 and approval["reviewed"] is True and approval["version"] == "0.1.7"
    source = approval["source_commit"]
    assert re.fullmatch(r"[0-9a-f]{40}", source)
    assert set(approval["package_sha256"]) == PACKAGES
    assert set(approval["artifacts"]) == ARTIFACTS | {approval["shared_artifact"]}
    run = api(f"repos/{REPO}/actions/runs/{int(approval['run_id'])}")
    assert run["head_sha"] == source and run["status"] == "completed" and run["conclusion"] == "success"
    assert run["path"] == ".github/workflows/full-release.yml" and run["event"] in {"push", "workflow_dispatch"}
    assert run["run_attempt"] == approval["run_attempt"]
    history = api(f"repos/{REPO}/actions/runs/{run['id']}/jobs?filter=all&per_page=100")["jobs"]
    latest = {j["name"]: j for j in sorted(history, key=lambda j: (j["started_at"] or "", j["id"]))}
    jobs = list(latest.values())
    assert len(jobs) == 5 and all(j["conclusion"] == "success" for j in jobs), "Every platform job must pass"
    listed = api(f"repos/{REPO}/actions/runs/{run['id']}/artifacts?per_page=100")["artifacts"]
    output = args.output.resolve()
    assert not output.exists(), "Use a new owned publication directory"
    output.mkdir(parents=True)
    unpacked = output / "artifacts"
    for name, reviewed in approval["artifacts"].items():
        artifact = next(a for a in listed if a["id"] == reviewed["id"])
        assert artifact["name"] == name and not artifact["expired"]
        assert artifact["digest"] == "sha256:" + reviewed["sha256"]
        archive_path = output / (name + ".zip")
        with archive_path.open("xb") as stream:
            command(["gh", "api", f"repos/{REPO}/actions/artifacts/{artifact['id']}/zip"], stdout=stream)
        assert digest(archive_path) == reviewed["sha256"]
        destination = unpacked / name
        with zipfile.ZipFile(archive_path) as archive:
            assert archive.testzip() is None
            for entry in archive.infolist():
                relative = PurePosixPath(entry.filename)
                assert not relative.is_absolute() and ".." not in relative.parts and "\\" not in entry.filename
                assert entry.file_size < 256 * 1024 * 1024
            archive.extractall(destination)
    shared = unpacked / approval["shared_artifact"]
    lock = json.loads(one(shared, "assets-lock.json").read_text())
    assert lock["schema"] == 2 and lock["source_commit"] == source
    assert str(lock["run_id"]) == str(run["id"]) and 0 < int(lock["run_attempt"]) <= run["run_attempt"]
    reports = {}
    for platform in ARTIFACTS:
        root = unpacked / platform
        if platform.startswith("full-macos"):
            report = json.loads(one(root, "validation.json").read_text())
            package = json.loads(one(root, "package.json").read_text())
            assert report["passed"] and report["source_commit"] == source
            assert report["gui"]["real_fits_texture_loaded"] and report["gui"]["owned_process_exited_after_cleanup"]
            assert package["source_commit"] == source and package["wasm_sha256"] == lock["wasm_sha256"]
            reports[platform] = {"validation": report, "package": package}
        elif platform == "full-linux-x86_64":
            report = json.loads(one(root, "linux-package.json").read_text())
            assert report["passed"] and report["source_commit"] == source and report["shared_wasm_matches"]
            reports[platform] = report
        else:
            reports[platform] = [json.loads(p.read_text()) for p in sorted(root.rglob("validation.json"))]
            assert len(reports[platform]) == 2 and all(p["passed"] for p in reports[platform])
            runtime = next(p for p in reports[platform] if "native_render_close_cycles" in p)
            assert len(runtime["native_render_close_cycles"]) == 3 and runtime["browser"]["passed"]
            assert runtime["histogram"]["passed"] and all(c["passed"] for c in runtime["histogram"]["checks"])
    release = output / "release"
    release.mkdir()
    for filename in sorted(PACKAGES):
        path = one(unpacked, filename)
        assert digest(path) == approval["package_sha256"][filename]
        (release / filename).write_bytes(path.read_bytes())
    # Release source comes directly from the exact commit used by every platform.
    source_path = release / "fitsviewer-0.1.7-source.tar.gz"
    with source_path.open("xb") as raw, gzip.GzipFile(filename="", mode="wb", fileobj=raw, mtime=0) as compressed:
        result = subprocess.run(["git", "archive", "--format=tar", "--prefix=fitsviewer-0.1.7/", source], stdout=subprocess.PIPE, check=True)
        compressed.write(result.stdout)
    reference = json.loads(subprocess.check_output(["git", "show", source + ":docs/releases/Linux-0.1.7-reference.json"], text=True))
    evidence = {"schema": 1, "version": "0.1.7", "source_commit": source, "run_url": run["html_url"],
                "common_linux_evidence": reference, "ci_resource_lock": lock, "platforms": reports,
                "limits": ["macOS ad-hoc signed; not notarized", "macOS 15 native startup is not complete interactive acceptance",
                           "Windows CI uses an external test-only software renderer", "External scientific packages remain external"]}
    (release / "VALIDATION-0.1.7.json").write_text(json.dumps(evidence, indent=2) + "\n")
    with zipfile.ZipFile(release / "VALIDATION-0.1.7-evidence.zip", "x", zipfile.ZIP_DEFLATED) as archive:
        for root in sorted(unpacked.iterdir()):
            if root.name not in ARTIFACTS:
                continue
            for path in sorted(root.rglob("*")):
                if path.is_file() and path.suffix in {".json", ".png"}:
                    archive.write(path, root.name + "/" + path.relative_to(root).as_posix())
    sums = release / "SHA256SUMS"
    sums.write_text("".join(digest(p) + "  " + p.name + "\n" for p in sorted(release.iterdir()) if p.is_file()))
    notes = output / "release-notes.md"
    notes.write_bytes(subprocess.check_output(["git", "show", source + ":docs/releases/v0.1.7.md"]))
    # Refuse to replace another release or move a previously existing tag.
    existing = subprocess.run(["gh", "release", "view", "v0.1.7", "--repo", REPO], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    assert existing.returncode != 0, "Release v0.1.7 already exists; inspect it instead of replacing assets"
    tag_check = subprocess.run(["gh", "api", f"repos/{REPO}/git/ref/tags/v0.1.7"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    assert tag_check.returncode != 0, "Tag v0.1.7 already exists; do not move it"
    command(["gh", "release", "create", "v0.1.7", "--repo", REPO, "--target", source,
             "--title", "FitsViewer 0.1.7 — Full", "--notes-file", notes, "--draft", *sorted(release.iterdir())])
    staged = api(f"repos/{REPO}/releases/tags/v0.1.7")
    assert staged["draft"] is True and staged["prerelease"] is False
    actual = {a["name"]: a for a in staged["assets"]}
    assert set(actual) == {p.name for p in release.iterdir()}
    for path in release.iterdir():
        assert actual[path.name]["size"] == path.stat().st_size
        assert actual[path.name]["digest"] == "sha256:" + digest(path)
    command(["gh", "release", "edit", "v0.1.7", "--repo", REPO, "--draft=false", "--latest"])
    published = api(f"repos/{REPO}/releases/tags/v0.1.7")
    assert published["draft"] is False and published["prerelease"] is False
    assert api(f"repos/{REPO}/git/ref/tags/v0.1.7")["object"]["sha"] == source
    (output / "publication-receipt.json").write_text(json.dumps({"published": True, "url": published["html_url"],
        "source_commit": source, "run_url": run["html_url"], "assets": {name: {k: a[k] for k in ["id", "name", "size", "digest"]}
        for name, a in actual.items()}}, indent=2) + "\n")
    print(json.dumps({"published": True, "url": published["html_url"], "assets": len(actual)}))


if __name__ == "__main__":
    main()
