"""Regression for Git EOL normalization without accepting stale science source.

行尾变化应通过；真实源码变化仍须拒绝。只检查资源，不编译程序。
"""
from pathlib import Path
import argparse
import json
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import build_macos_full as mac
from macos_licenses import collect_from_metadata, verify_inventory, sha256


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", type=Path, required=True)
    args = parser.parse_args()
    wheel = args.wheel.resolve()
    mac.validate_wheel(wheel, mac.lock())
    parent = ROOT / "build/macos-packaging-tests"
    parent.mkdir(parents=True, exist_ok=True)
    original_root = mac.ROOT
    with tempfile.TemporaryDirectory(prefix="owned-", dir=parent) as temporary:
        work = Path(temporary)
        assert work.resolve().is_relative_to(parent.resolve())
        simulated = work / "checkout"
        adapters = simulated / "web/python"
        adapters.mkdir(parents=True)
        for source in (ROOT / "web/python").glob("*.py"):
            # Reproduce Linux/macOS checkout bytes rather than Windows autocrlf.
            blob = subprocess.check_output(["git", "show", "HEAD:web/python/" + source.name], cwd=ROOT)
            (adapters / source.name).write_bytes(blob.replace(b"\r\n", b"\n"))
        mac.ROOT = simulated
        try:
            mac.extract_resources(wheel, work / "accepted")
            changed = adapters / "fitsviewer_science.py"
            current = changed.read_bytes()
            assert b"MAX_INPUT=512*1024*1024" in current
            changed.write_bytes(current.replace(b"MAX_INPUT=512*1024*1024", b"MAX_INPUT=512*1024*1024-1", 1))
            try:
                mac.extract_resources(wheel, work / "rejected")
            except ValueError as error:
                assert str(error) == "Shared scientific adapter is stale: fitsviewer_science.py"
            else:
                raise AssertionError("A non-line-ending source change was accepted")
        finally:
            mac.ROOT = original_root
        # License coverage follows normal/build edges and preserves original text.
        # 真实依赖必须有原始许可；缺失、篡改许可均应阻断打包。
        legal_root = work / "legal-source"
        legal_root.mkdir()
        (legal_root / "Cargo.lock").write_bytes((ROOT / "Cargo.lock").read_bytes())
        dependency = work / "native-binding"
        dependency.mkdir()
        original_license = (ROOT / "LICENSE").read_bytes()
        (dependency / "LICENSE-MIT").write_bytes(original_license)
        package = {"id": "native", "name": "fixture-native", "version": "1.0.0",
                   "manifest_path": str(dependency / "Cargo.toml"), "license": "MIT", "license_file": None,
                   "authors": ["Fixture native maintainer"], "repository": "https://example.invalid/native", "source": "fixture"}
        metadata = {"workspace_members": ["host"],
                    "packages": [{"id": "host", "name": "fitsviewer"}, package,
                                 {"id": "dev", "name": "unused-dev", "version": "1.0.0"}],
                    "resolve": {"nodes": [
                        {"id": "host", "deps": [{"pkg": "native", "dep_kinds": [{"kind": None}]},
                                                   {"pkg": "dev", "dep_kinds": [{"kind": "dev"}]}]},
                        {"id": "native", "deps": []}, {"id": "dev", "deps": []}]}}
        resources = work / "legal-package"
        resources.mkdir()
        inventory = collect_from_metadata(metadata, legal_root, resources, "aarch64-apple-darwin", fetch_missing=False)
        assert len(inventory["packages"]) == 1
        assert inventory["packages"][0]["authors"] == package["authors"]
        copied = resources / inventory["packages"][0]["primary_files"][0]
        assert copied.read_bytes() == original_license
        assert verify_inventory(resources, "aarch64-apple-darwin", sha256(legal_root / "Cargo.lock")) == 1
        copied.write_bytes(original_license + b"changed")
        try:
            verify_inventory(resources, "aarch64-apple-darwin", sha256(legal_root / "Cargo.lock"))
        except ValueError as error:
            assert "absent or changed" in str(error)
        else:
            raise AssertionError("Changed native license text was accepted")
        (dependency / "LICENSE-MIT").unlink()
        try:
            collect_from_metadata(metadata, legal_root, work / "unlicensed", "aarch64-apple-darwin", fetch_missing=False)
        except ValueError as error:
            assert str(error) == "Missing original native dependency license: fixture-native-1.0.0"
        else:
            raise AssertionError("An unlicensed active native dependency was accepted")
        # The exact dispatch exception must not become a generic MIT fallback.
        shutil.copytree(ROOT / "licenses/macos", legal_root / "licenses/macos")
        dispatch_source = work / "dispatch-source"
        dispatch_source.mkdir()
        original_supplement = ROOT / "licenses/macos/dispatch-0.2.0"
        shutil.copy2(original_supplement / "Cargo.toml.orig", dispatch_source / "Cargo.toml.orig")
        shutil.copy2(original_supplement / "cargo-vcs-info.json", dispatch_source / ".cargo_vcs_info.json")
        dispatch_package = {**package, "name": "dispatch", "version": "0.2.0", "authors": ["Steven Sheldon"],
                            "manifest_path": str(dispatch_source / "Cargo.toml")}
        dispatch_metadata = json.loads(json.dumps(metadata))
        dispatch_metadata["packages"][1] = dispatch_package
        dispatch_inventory = collect_from_metadata(dispatch_metadata, legal_root, work / "dispatch-accepted",
                                                   "aarch64-apple-darwin", fetch_missing=False)
        assert dispatch_inventory["packages"][0]["legal_note"]
        (dispatch_source / "Cargo.toml.orig").write_bytes(b"changed declaration")
        try:
            collect_from_metadata(dispatch_metadata, legal_root, work / "dispatch-rejected",
                                  "aarch64-apple-darwin", fetch_missing=False)
        except ValueError as error:
            assert "original declaration differs" in str(error)
        else:
            raise AssertionError("A changed dispatch original declaration was accepted")
    print(json.dumps({"passed": True, "git_lf_vs_wheel_crlf_accepted": True,
                      "changed_scientific_limit_rejected": True, "original_native_license_and_authors_preserved": True,
                      "dev_only_dependency_excluded": True, "changed_license_rejected": True,
                      "missing_active_dependency_license_rejected": True,
                      "exact_dispatch_declaration_and_standard_text_accepted": True,
                      "changed_dispatch_declaration_rejected": True}))


if __name__ == "__main__":
    main()
