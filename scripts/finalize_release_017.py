"""Finish the existing reviewed 0.1.7 draft by release ID, without reuploading.

Only small evidence assets are downloaded. Application assets are checked against
their authenticated GitHub digests and the reviewed schema-3 approval.
"""
from pathlib import Path, PurePosixPath
import argparse
import gzip
import hashlib
import io
import json
import os
import re
import stat
import subprocess
import time
import zipfile

import publish_reviewed_017 as publisher
import ui_only_release as ui

REPO = publisher.REPO
SOURCE = "841fca5ceede115e129fcf52d8a3ec0aae2b8cc0"
RUN = 40977438784
TAG = "v0.1.7"
TITLE = "FitsViewer 0.1.7 — Full"
EXTRA = {"fitsviewer-0.1.7-source.tar.gz", "VALIDATION-0.1.7.json",
         "VALIDATION-0.1.7-evidence.zip", "SHA256SUMS"}


def api(path, payload=None, optional=False):
    command = ["gh", "api", "repos/" + REPO + "/" + path]
    if payload is not None:
        command += ["--method", "PATCH", "--input", "-"]
    result = subprocess.run(command, input=json.dumps(payload) if payload is not None else None,
                            text=True, capture_output=True)
    if result.returncode:
        if optional:
            if re.search(r"\bHTTP 404\b", result.stderr):
                return None
            try:
                if str(json.loads(result.stdout).get("status")) == "404":
                    return None
            except (ValueError, AttributeError):
                pass
        raise RuntimeError(result.stderr.strip() or result.stdout.strip())
    return json.loads(result.stdout)


def source_blob(path):
    return subprocess.check_output(["git", "show", SOURCE + ":" + path])


def sha(data):
    return hashlib.sha256(data).hexdigest()


def read_asset(asset):
    assert 0 < asset["size"] <= 64 * 1024 * 1024, "Evidence asset size exceeds the bounded recovery scope"
    data = subprocess.check_output(["gh", "api", "-H", "Accept: application/octet-stream",
        f"repos/{REPO}/releases/assets/{asset['id']}"])
    assert len(data) == asset["size"] and "sha256:" + sha(data) == asset["digest"]
    return data


def asset_map(release):
    assets = release["assets"]
    assert len(assets) == len({a["name"] for a in assets})
    result = {a["name"]: a for a in assets}
    assert set(result) == publisher.PACKAGES | EXTRA
    assert all(a["state"] == "uploaded" and a["size"] > 0 and
               re.fullmatch(r"sha256:[0-9a-f]{64}", a["digest"]) for a in assets)
    return result


def verify_ci(approval, record):
    run = api(f"actions/runs/{RUN}")
    assert run["head_sha"] == SOURCE and run["path"] == ".github/workflows/full-release.yml"
    assert run["status"] == "completed" and run["conclusion"] == "success"
    assert run["run_attempt"] == approval["run_attempt"]
    latest = ui.origin.latest_jobs(api(f"actions/runs/{RUN}/jobs?filter=all&per_page=100")["jobs"])
    assert len(latest) == 5 and all(j["conclusion"] == "success" for j in latest.values())
    for name, expected in approval["artifacts"].items():
        artifact = api(f"actions/artifacts/{expected['id']}")
        assert artifact["name"] == name and not artifact["expired"]
        assert artifact["digest"] == "sha256:" + expected["sha256"]
        assert artifact["workflow_run"]["id"] == RUN and artifact["workflow_run"]["head_sha"] == SOURCE
    ui.verify_origin_metadata(record)
    return run


def verify_validation(value, approval, record, targeted, run):
    assert value["schema"] == 1 and value["version"] == "0.1.7"
    assert value["source_commit"] == SOURCE and value["run_url"] == run["html_url"]
    assert value["validation_mode"] == "ui_only" and value["ui_record"] == approval["ui_record"]
    assert value["common_linux_evidence"] == json.loads(source_blob("docs/releases/Linux-0.1.7-reference.json"))
    lock = value["ci_resource_lock"]
    assert lock["schema"] == 2 and lock["origin"] == "github-actions-same-run"
    assert lock["repository"] == REPO and lock["source_commit"] == SOURCE
    assert int(lock["run_id"]) == RUN and 0 < int(lock["run_attempt"]) <= run["run_attempt"]
    assert re.fullmatch(r"[0-9a-f]{64}", lock["wasm_sha256"])
    reports = value["platforms"]
    assert set(reports) == publisher.ARTIFACTS | {"scope"}
    for name, report in reports.items():
        assert report["passed"] is True and report["ui_only"] is True
        assert report["source_commit"] == SOURCE and report["baseline_source"] == ui.origin.SOURCE
        assert report["run_id"] == RUN and 0 < report["run_attempt"] <= run["run_attempt"]
        assert report["record_path"] == approval["ui_record"]["path"]
        assert report["record_sha256"] == approval["ui_record"]["sha256"]
        assert report["hash_convention"] == "sha256-lf"
        assert report["runtime_sha256"] == record["runtime_sha256"] and report["targeted"] == targeted
        if name == "scope":
            assert report["kind"] == "ui-only-scope-0.1.7" and report["origin_artifact_metadata_verified"] is True
        else:
            assert report["kind"] == "ui-only-package-0.1.7" and report["platform"] == name.removeprefix("full-")
            assert report["build_info"] == {"schema": 1, "version": "0.1.7", "features": ui.FEATURES}
            assert report["shared_wasm_matches"] is True and report["wasm_sha256"] == lock["wasm_sha256"]
            assert report["architecture_checked"] is True and re.fullmatch(r"[0-9a-f]{64}", report["native_binary_sha256"])
            expected = ({n for n in publisher.PACKAGES if n.endswith(("win_amd64.whl", "setup.exe", "windows-x86_64-portable.zip"))}
                        if name == "full-windows-x86_64" else
                        {n for n in publisher.PACKAGES if (n.endswith("manylinux_2_28_x86_64.whl")
                            if name == "full-linux-x86_64" else n.endswith(name.removeprefix("full-") + "-portable.zip"))})
            assert set(report["packages"]) == expected
    packages = {name: checksum for key, report in reports.items() if key != "scope"
                for name, checksum in report["packages"].items()}
    assert packages == approval["package_sha256"]
    history = value["historical_functional_evidence"]
    old = record["origin_evidence"]
    assert history["source_commit"] == old["source_commit"] == ui.origin.SOURCE
    assert history["run_url"] == f"https://github.com/{REPO}/actions/runs/{old['run_id']}"
    assert history["supplemental_windows_run_url"] == f"https://github.com/{REPO}/actions/runs/{old['supplemental_windows']['run_id']}"
    assert history["artifact_sha256"] == old["artifacts"]
    provenance = history["supplemental_windows_provenance"]
    assert provenance["source_commit"] == ui.origin.SOURCE
    assert provenance["driver_commit"] == old["supplemental_windows"]["driver_commit"]
    assert provenance["run_id"] == old["supplemental_windows"]["run_id"]
    assert provenance["origin_run_id"] == old["run_id"]
    assert provenance["packages"] == {n: h for n, h in old["package_sha256"].items()
        if n.endswith(("win_amd64.whl", "setup.exe", "windows-x86_64-portable.zip"))}


def verify_evidence(data, value, record):
    """Bind the JSON/PNG-only evidence archive to the semantic summary."""
    records, hashes = {}, {}
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        assert archive.testzip() is None
        seen = set()
        for entry in archive.infolist():
            path = PurePosixPath(entry.filename)
            assert not path.is_absolute() and ".." not in path.parts and "\\" not in entry.filename and ":" not in entry.filename
            assert not entry.is_dir() and not stat.S_ISLNK(entry.external_attr >> 16)
            assert entry.filename.casefold() not in seen and entry.file_size <= 32 * 1024 * 1024
            seen.add(entry.filename.casefold())
            parts = path.parts
            platform = parts[1] if parts[0] == "origin" else parts[0]
            assert platform in publisher.ARTIFACTS and path.suffix in {".json", ".png"}
            raw = archive.read(entry)
            if path.suffix == ".json":
                records[entry.filename] = json.loads(raw)
                hashes[entry.filename] = sha(raw)
                if isinstance(records[entry.filename], dict) and "source_commit" in records[entry.filename]:
                    assert records[entry.filename]["source_commit"] == (ui.origin.SOURCE if parts[0] == "origin" else SOURCE)
            else:
                assert raw.startswith(b"\x89PNG\r\n\x1a\n")

    def one(prefix, basename):
        found = [item for path, item in records.items() if path.startswith(prefix + "/") and PurePosixPath(path).name == basename]
        assert len(found) == 1, (prefix, basename)
        return found[0]

    for name in publisher.ARTIFACTS:
        assert one(name, "ui-package.json") == value["platforms"][name]
        if name.startswith("full-macos"):
            package = one(name, "package.json")
            report = value["platforms"][name]
            assert package["source_commit"] == SOURCE and package["native_binary_sha256"] == report["native_binary_sha256"]
            assert package["wasm_sha256"] == report["wasm_sha256"]
            assert report["packages"][package["portable"]] == package["portable_sha256"]
    old = value["historical_functional_evidence"]
    assert set(old["platforms"]) == publisher.ARTIFACTS
    for name, report in old["platforms"].items():
        prefix = "origin/" + name
        if name.startswith("full-macos"):
            assert one(prefix, "validation.json") == report["validation"]
            assert one(prefix, "package.json") == report["package"]
            assert report["validation"]["passed"] is True and report["validation"]["source_commit"] == ui.origin.SOURCE
            assert report["validation"]["gui"]["real_fits_texture_loaded"] is True
            assert report["validation"]["gui"]["owned_process_exited_after_cleanup"] is True
            assert report["package"]["source_commit"] == ui.origin.SOURCE
            assert report["package"]["wasm_sha256"] == old["supplemental_windows_provenance"]["resources"]["wasm_sha256"]
        elif name == "full-linux-x86_64":
            assert one(prefix, "linux-package.json") == report
            assert report["passed"] is True and report["source_commit"] == ui.origin.SOURCE and report["shared_wasm_matches"] is True
        else:
            reports = [item for path, item in records.items() if path.startswith(prefix + "/") and PurePosixPath(path).name == "validation.json"]
            assert sorted(map(lambda item: json.dumps(item, sort_keys=True), reports)) == sorted(map(lambda item: json.dumps(item, sort_keys=True), report))
            assert len(report) == 2 and all(item["passed"] is True for item in report)
            runtime = next(item for item in report if "native_render_close_cycles" in item)
            assert len(runtime["native_render_close_cycles"]) == 3 and runtime["browser"]["passed"] is True
            assert runtime["histogram"]["passed"] is True and all(item["passed"] is True for item in runtime["histogram"]["checks"])
            setup = next(item for item in report if "installed_payload_integrity" in item)
            assert setup["installed_payload_integrity"]["sha256_matches"] is True
            assert one(prefix, "provenance.json") == old["supplemental_windows_provenance"]
            proof_hashes = [checksum for path, checksum in hashes.items()
                            if path.startswith(prefix + "/") and PurePosixPath(path).name == "provenance.json"]
            assert proof_hashes == [record["origin_evidence"]["supplemental_windows"]["provenance_sha256"]]
    return {"entries": len(seen), "json_records": len(records)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--approval", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--execute", action="store_true")
    args = parser.parse_args()
    approval = json.loads(args.approval.read_text())
    assert approval["schema"] == 3 and approval["reviewed"] is True and approval["ui_only"] is True
    assert approval["resume_existing_draft"] is True and approval["source_commit"] == SOURCE
    assert approval["version"] == "0.1.7" and approval["run_id"] == RUN
    assert set(approval["package_sha256"]) == publisher.PACKAGES
    assert set(approval["artifacts"]) == publisher.ARTIFACTS | {f"full-shared-{RUN}"}
    assert approval["ui_record"]["path"] == "qa/ui-only-017.json"
    if not args.execute:
        print(json.dumps({"will_publish": False, "source_commit": SOURCE, "resume_existing_draft": True}))
        return
    assert os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REPOSITORY") == REPO
    assert os.environ.get("GITHUB_REF") == "refs/heads/codex/publish-0.1.7"
    raw = source_blob(approval["ui_record"]["path"])
    assert ui.source_sha(raw) == approval["ui_record"]["sha256"]
    record = json.loads(raw)
    targeted = ui.record_check(record, approval["ui_record"]["path"], SOURCE)
    run = verify_ci(approval, record)
    candidates = []
    for page in range(1, 101):
        releases = api(f"releases?per_page=100&page={page}")
        candidates += [r for r in releases if r["tag_name"] == TAG or
                       (r["name"] == TITLE and r["target_commitish"] == SOURCE)]
        if len(releases) < 100:
            break
    else:
        raise AssertionError("Release list exceeds this bounded recovery scope")
    assert len(candidates) == 1, "Expected one matching draft or already-published release; inspect ambiguity"
    release = api(f"releases/{candidates[0]['id']}")
    assert release["name"] == TITLE and release["target_commitish"] == SOURCE and release["prerelease"] is False
    if not release["draft"]:
        assert release["tag_name"] == TAG
    assets = asset_map(release)
    for name, checksum in approval["package_sha256"].items():
        assert assets[name]["digest"] == "sha256:" + checksum
    archive = subprocess.check_output(["git", "archive", "--format=tar", "--prefix=fitsviewer-0.1.7/", SOURCE])
    buffer = io.BytesIO()
    with gzip.GzipFile(filename="", mode="wb", fileobj=buffer, mtime=0) as compressed:
        compressed.write(archive)
    assert assets["fitsviewer-0.1.7-source.tar.gz"]["digest"] == "sha256:" + sha(buffer.getvalue())
    summary_raw = read_asset(assets["VALIDATION-0.1.7.json"])
    summary = json.loads(summary_raw)
    verify_validation(summary, approval, record, targeted, run)
    evidence_raw = read_asset(assets["VALIDATION-0.1.7-evidence.zip"])
    evidence = verify_evidence(evidence_raw, summary, record)
    checksums = {}
    for line in read_asset(assets["SHA256SUMS"]).decode("utf-8").splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  ([^/\\]+)", line)
        assert match and match[2] not in checksums
        checksums[match[2]] = match[1]
    assert set(checksums) == set(assets) - {"SHA256SUMS"}
    assert all(assets[name]["digest"] == "sha256:" + checksum for name, checksum in checksums.items())
    tag = api("git/ref/tags/" + TAG, optional=True)
    assert tag is None or (tag["object"]["type"] == "commit" and tag["object"]["sha"] == SOURCE)
    was_draft = release["draft"]
    if was_draft:
        api(f"releases/{release['id']}", {"draft": False, "make_latest": "true", "tag_name": TAG,
                                       "target_commitish": SOURCE})
    published = api(f"releases/{release['id']}")
    assert published["draft"] is False and published["prerelease"] is False
    assert published["tag_name"] == TAG and published["target_commitish"] == SOURCE
    assert published["html_url"] == f"https://github.com/{REPO}/releases/tag/{TAG}"
    final_assets = asset_map(published)
    for name in assets:
        assert all(final_assets[name][key] == assets[name][key] for key in ["id", "name", "size", "digest"])
    for attempt in range(6):
        tag = api("git/ref/tags/" + TAG, optional=True)
        if tag:
            break
        time.sleep(1)
    assert tag and tag["object"]["type"] == "commit" and tag["object"]["sha"] == SOURCE
    args.output.mkdir(parents=True, exist_ok=True)
    receipt = {"published": True, "resumed_existing_release": True, "previously_draft": was_draft,
               "release_id": published["id"], "url": published["html_url"], "source_commit": SOURCE,
               "run_url": run["html_url"], "ui_record": approval["ui_record"], "evidence": evidence,
               "assets": {name: {key: asset[key] for key in ["id", "name", "size", "digest"]}
                          for name, asset in final_assets.items()}}
    (args.output / "publication-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"published": True, "url": published["html_url"], "release_id": published["id"],
                      "assets": len(final_assets), "previously_draft": was_draft}))


if __name__ == "__main__":
    main()
