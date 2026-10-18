"""Relocated helper entry uses shared source and external Python, without Jupyter.

检查 Mac 打包入口的可移动性与依赖边界；不冒充真实 macOS 原生或 SSH GUI 验收。
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from build_macos_full import stage_remote_entry


class PortableHelperEntry(unittest.TestCase):
    def test_relocated_entry_loads_shared_code_and_rejects_invalid_endpoint(self):
        parent = ROOT / "target/qa/remote-helper"
        if os.name == "nt":
            parent = Path("\\\\?\\" + str(parent.resolve()))
        parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="owned-", dir=parent) as temporary:
            payload = Path(temporary) / "移动目录 with spaces"
            resources = payload / "FitsViewer.app/Contents/Resources"
            resources.mkdir(parents=True)
            stage_remote_entry(resources, payload)
            package = resources / "python/fitsviewer"
            for name in ["__init__.py", "build_info.py", "runtime.py", "remote.py"]:
                self.assertEqual((package / name).read_bytes(), (ROOT / "jupyter/fitsviewer" / name).read_bytes())
            # Even hostile inherited package search settings cannot load another helper.
            environment = dict(os.environ, PYTHONPATH=str(Path(temporary) / "absent"))
            entry = [sys.executable, "-I", str(resources / "fver.py")]
            help_result = subprocess.run(entry + ["--help"], env=environment, capture_output=True, text=True, timeout=15)
            self.assertEqual(help_result.returncode, 0, help_result.stderr)
            self.assertIn("--target", help_result.stdout)
            self.assertIn("--ssh", help_result.stdout)
            invalid = subprocess.run(entry + ["helper", "--url", "https://example.invalid"],
                                     env=environment, capture_output=True, text=True, timeout=15)
            self.assertEqual(invalid.returncode, 1, invalid.stderr)
            self.assertIn("loopback", invalid.stderr)
            self.assertEqual(json.loads((package / "build-profile.json").read_text())["profile"], "full")
            self.assertNotIn("_vendor", {p.name for p in package.iterdir()})
            self.assertFalse(any(package.rglob("__pycache__")), "A signed app must not write bytecode into its resources")


if __name__ == "__main__":
    unittest.main(verbosity=2)
