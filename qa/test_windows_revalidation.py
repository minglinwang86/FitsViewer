"""Offline checks for Windows-only rerun provenance; no builds or GitHub writes."""
from pathlib import Path
import copy
import hashlib
import json
import sys
import tempfile
import unittest
import zipfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import stage_windows_revalidation as stage
import publish_reviewed_017 as publish


class Revalidation(unittest.TestCase):
    def setUp(self):
        self.origin = {"id": stage.ORIGIN_RUN, "head_sha": stage.SOURCE,
            "path": ".github/workflows/full-release.yml", "event": "push",
            "status": "completed", "conclusion": "failure", "run_attempt": 1}
        self.jobs = [{"id": i, "name": name, "started_at": "", "conclusion": "success"}
            for i, name in enumerate(sorted(stage.GOOD_JOBS))]
        self.jobs.append({"id": 99, "name": stage.ORIGIN_WINDOWS, "started_at": "", "conclusion": "failure",
                          "steps": [{"name": stage.ORIGIN_NATIVE_STEP, "conclusion": "success"}]})
        self.extra = {"id": 123456, "head_sha": "b"*40, "head_branch": "codex/windows-revalidate-017",
            "path": stage.WORKFLOW, "event": "push", "status": "completed",
            "conclusion": "success", "run_attempt": 1}
        self.approval = {"schema": 2, "source_commit": stage.SOURCE,
            "shared_artifact": stage.SHARED_NAME,
            "artifacts": {stage.SHARED_NAME: {"id": stage.SHARED_ID, "sha256": stage.SHARED_SHA}},
            "supplemental_windows": {"run_id": self.extra["id"], "run_attempt": 1,
                "driver_commit": self.extra["head_sha"], "provenance_sha256": "c"*64,
                "origin_shared_artifact": {"id": stage.SHARED_ID, "name": stage.SHARED_NAME, "sha256": stage.SHARED_SHA}}}
        self.extra_jobs = [{"id": 100, "name": stage.WINDOWS_JOB, "started_at": "", "conclusion": "success"}]

    def api(self, path):
        return {"jobs": self.extra_jobs} if "/jobs?" in path else self.extra

    def test_only_windows_failed_origin_is_accepted(self):
        stage.validate_origin(self.origin, self.jobs)

    def test_other_failures_success_wrong_source_and_extra_jobs_rejected(self):
        for field, value in [("conclusion", "success"), ("head_sha", "0"*40), ("id", 1)]:
            with self.subTest(field=field), self.assertRaises(AssertionError):
                stage.validate_origin({**self.origin, field: value}, self.jobs)
        bad = copy.deepcopy(self.jobs);bad[0]["conclusion"] = "failure"
        with self.assertRaises(AssertionError):stage.validate_origin(self.origin, bad)
        with self.assertRaises(AssertionError):stage.validate_origin(self.origin, self.jobs + [
            {"id": 999, "name": "Unexpected", "started_at": "", "conclusion": "success"}])
        bad = copy.deepcopy(self.jobs);bad[-1]["steps"][0]["conclusion"] = "failure"
        with self.assertRaises(AssertionError):stage.validate_origin(self.origin, bad)

    def test_explicit_successful_supplement_accepted(self):
        with patch.object(publish, "api", side_effect=self.api):
            self.assertEqual(publish.supplemental_run(self.approval, self.origin, self.jobs), self.extra)

    def test_supplement_wrong_driver_failure_workflow_or_attempt_rejected(self):
        for field, value in [("head_sha", stage.SOURCE), ("conclusion", "failure"),
                            ("path", ".github/workflows/full-release.yml"), ("run_attempt", 2),
                            ("head_branch", "main")]:
            with self.subTest(field=field), patch.object(publish, "api", side_effect=lambda p: (
                {"jobs": self.extra_jobs} if "/jobs?" in p else {**self.extra, field: value})):
                with self.assertRaises(AssertionError):
                    publish.supplemental_run(self.approval, self.origin, self.jobs)

    def test_supplement_cannot_replace_other_origin_or_resource_archive(self):
        for value in ["0"*64, "1"*64]:
            approval = copy.deepcopy(self.approval)
            approval["supplemental_windows"]["origin_shared_artifact"]["sha256"] = value
            with self.assertRaises(AssertionError):
                publish.supplemental_run(approval, self.origin, self.jobs)

    def test_provenance_binds_source_native_qa_resources_and_package(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary);native = "100644 blob abc\tcrates/server/src/main.rs"
            qa = b"corrected QA\n";binary = b"native";wasm = b"wasm"
            lock = {"wheel_sha256": "d"*64, "wasm_sha256": hashlib.sha256(wasm).hexdigest()}
            wheel = root/"fitsviewer-0.1.7-py3-none-win_amd64.whl"
            with zipfile.ZipFile(wheel, "w") as z:
                z.writestr("fitsviewer/_vendor/web/pkg/fitsviewer_web_bg.wasm", wasm)
                z.writestr("fitsviewer/_vendor/bin/fitsviewer.exe", binary)
            self.approval["package_sha256"] = {wheel.name: publish.digest(wheel),
                "FitsViewer-0.1.7-windows-x86_64-setup.exe": "1"*64,
                "FitsViewer-0.1.7-windows-x86_64-portable.zip": "2"*64}
            proof = {"schema": 1, "kind": "supplemental-windows-0.1.7", "source_commit": stage.SOURCE,
                "driver_commit": self.extra["head_sha"], "driver_tree": "driver-tree",
                "application_tree": "application-tree", "run_id": self.extra["id"], "run_attempt": 1,
                "origin_run_id": self.origin["id"], "origin_run_attempt": 1,
                "origin_shared_artifact": self.approval["supplemental_windows"]["origin_shared_artifact"],
                "resources": lock, "runtime_tracked_unchanged": True,
                "reused_windows_ipc": {"origin_run_id": stage.ORIGIN_RUN, "step": stage.ORIGIN_NATIVE_STEP,
                                       "conclusion": "success", "source_commit": stage.SOURCE},
                "build_info": {"schema": 1, "version": "0.1.7", "features": {"base": True, "pro": True, "desktop": True}},
                "native_source_blobs_sha256": hashlib.sha256(native.encode()).hexdigest(),
                "qa_override": {"path": "qa/histogram_window.cjs", "sha256_lf": hashlib.sha256(qa).hexdigest()},
                "packages": self.approval["package_sha256"], "native_binary_sha256": hashlib.sha256(binary).hexdigest()}
            (root/"native-source-blobs.txt").write_text(native+"\n")
            def git(args, **kwargs):
                if args[1] == "show":return qa
                if args[1] == "ls-tree":return native
                return ("application-tree" if args[2].startswith(stage.SOURCE) else "driver-tree")
            def verify(value):
                file = root/"provenance.json";file.write_text(json.dumps(value))
                self.approval["supplemental_windows"]["provenance_sha256"] = publish.digest(file)
                return publish.verify_supplemental_provenance(self.approval, self.origin, self.extra, root, lock)
            with patch.object(publish.subprocess, "check_output", side_effect=git):
                self.assertEqual(verify(proof), proof)
                for field, value in [("source_commit", "a"*40), ("runtime_tracked_unchanged", False),
                                     ("native_binary_sha256", "0"*64), ("native_source_blobs_sha256", "0"*64)]:
                    bad = {**proof, field: value}
                    with self.subTest(field=field), self.assertRaises(AssertionError):verify(bad)
                bad = copy.deepcopy(proof);bad["resources"]["wasm_sha256"] = "0"*64
                # Copy the lock so a deliberately altered proof cannot mutate the oracle.
                with self.assertRaises(AssertionError):verify(bad)


if __name__ == "__main__":
    unittest.main()

