"""Reject CI resources from another source/run and preserve release locks."""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import build_macos_full as mac


class ResourceIdentity(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="fitsviewer-resource-lock-")
        self.path = Path(self.temporary.name) / "lock.json"
        self.revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        self.value = {"schema": 2, "origin": "github-actions-same-run", "repository": "minglinwang86/FitsViewer",
                      "source_commit": self.revision, "run_id": "12345", "run_attempt": "2", "version": "0.1.7",
                      "wheel_asset": "resources.whl", "wheel_bytes": 4,
                      "wheel_sha256": hashlib.sha256(b"test").hexdigest(), "wasm_sha256": "0" * 64}
        self.environment = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": self.value["repository"],
                            "GITHUB_SHA": self.revision, "GITHUB_RUN_ID": "12345", "GITHUB_RUN_ATTEMPT": "2"}

    def tearDown(self):
        self.temporary.cleanup()

    def read(self, value=None, environment=None):
        self.path.write_text(json.dumps(value or self.value), encoding="utf-8")
        with patch.object(mac, "LOCK_PATH", self.path), patch.dict(os.environ, environment or self.environment):
            return mac.lock()

    def test_same_source_and_run_accepted(self):
        self.assertEqual(self.read(), self.value)

    def test_failed_platform_retry_reuses_earlier_same_run_resources(self):
        earlier = {**self.value, "run_attempt": "1"}
        self.assertEqual(self.read(earlier), earlier)

    def test_future_or_zero_attempt_rejected(self):
        for attempt in ["0", "3", "-1"]:
            with self.subTest(attempt=attempt), self.assertRaisesRegex(ValueError, "another run or source"):
                self.read({**self.value, "run_attempt": attempt})

    def test_other_run_or_attempt_rejected(self):
        for field in ["run_id", "run_attempt", "repository", "source_commit"]:
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, "another run or source|Unexpected shared"):
                self.read({**self.value, field: "different"})

    def test_checkout_mismatch_rejected_even_if_environment_matches(self):
        with self.assertRaisesRegex(ValueError, "source commit differs"):
            self.read({**self.value, "source_commit": "0" * 40}, {**self.environment, "GITHUB_SHA": "0" * 40})

    def test_invalid_hash_and_changed_archive_rejected(self):
        with self.assertRaisesRegex(ValueError, "Invalid locked SHA256"):
            self.read({**self.value, "wasm_sha256": "bad"})
        wheel = self.path.with_suffix(".whl")
        wheel.write_bytes(b"test")
        mac.validate_wheel(wheel, self.read())
        wheel.write_bytes(b"edit")
        with self.assertRaisesRegex(ValueError, "differs from the reviewed lock"):
            mac.validate_wheel(wheel, self.read())

    def test_default_release_lock_is_preserved(self):
        with patch.object(mac, "LOCK_PATH", ROOT / "config/macos-full-assets.json"):
            value = mac.lock()
        self.assertEqual(value["schema"], 1)
        self.assertGreater(value["release_id"], 0)
        self.assertGreater(value["asset_id"], 0)


if __name__ == "__main__":
    unittest.main()
