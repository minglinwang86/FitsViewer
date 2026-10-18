"""Release installation regression: select matching Full assets without importing Jupyter."""
import importlib
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "jupyter"))
install = importlib.import_module("fitsviewer.install")


class FullWheelSelection(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        (self.root / "jupyter").mkdir()
        (self.root / "jupyter/pyproject.toml").write_text('[project]\nversion = "0.1.5rc1"\n')
        self.addCleanup(self.temporary.cleanup)

    def wheel(self, tag, directory="dist/full", version="0.1.5rc1"):
        path = self.root / directory / ("fitsviewer-" + version + "-py3-none-" + tag + ".whl")
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"fixture")
        return path

    @patch.object(install.sys, "platform", "linux")
    @patch.object(install.platform, "machine", return_value="x86_64")
    def test_profile_directory_version_and_architecture(self, machine):
        expected = self.wheel("manylinux_2_28_x86_64")
        self.wheel("win_amd64")
        self.wheel("manylinux_2_28_aarch64")
        self.wheel("linux_x86_64", version="0.1.4rc2")
        self.assertEqual(install._source_wheel(self.root), expected)

    @patch.object(install.sys, "platform", "linux")
    @patch.object(install.platform, "machine", return_value="x86_64")
    def test_repaired_artifact_preferred_and_raw_excluded(self, machine):
        expected = self.wheel("manylinux_2_28_x86_64")
        self.wheel("linux_x86_64")
        self.wheel("linux_x86_64", directory="dist/full/raw")
        self.assertEqual(install._source_wheel(self.root), expected)

    @patch.object(install.sys, "platform", "win32")
    @patch.object(install.platform, "machine", return_value="AMD64")
    def test_windows_excludes_linux(self, machine):
        expected = self.wheel("win_amd64")
        self.wheel("manylinux_2_28_x86_64")
        self.assertEqual(install._source_wheel(self.root), expected)

    @patch.object(install.sys, "platform", "linux")
    @patch.object(install.platform, "machine", return_value="x86_64")
    def test_no_cross_profile_fallback(self, machine):
        self.wheel("linux_x86_64", directory="dist/mini")
        self.wheel("linux_x86_64", directory="dist/full/raw")
        with self.assertRaises(FileNotFoundError):
            install._source_wheel(self.root)

    @patch.object(install.sys, "platform", "linux")
    @patch.object(install.platform, "machine", return_value="x86_64")
    def test_explicit_legacy_directory(self, machine):
        expected = self.wheel("linux_x86_64", directory="dist")
        self.assertEqual(install._source_wheel(self.root), expected)


if __name__ == "__main__":
    unittest.main()
