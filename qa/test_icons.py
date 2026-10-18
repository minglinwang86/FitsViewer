"""Small icon/entry-point regressions; no builds, services or desktop changes.
图标及入口回归；不构建、不启动服务、不修改用户桌面。
"""
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import ast
import importlib
import importlib.util
import json
import os
import plistlib
import shutil
import struct
import sys
import tempfile
import unittest
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "scripts"))
import build_profiles as profiles


class IconIntegration(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.name = "fitsviewer_icon_test"
        spec = importlib.util.spec_from_file_location(cls.name, ROOT / "jupyter/fitsviewer/__init__.py", submodule_search_locations=[str(ROOT / "jupyter/fitsviewer")])
        module = importlib.util.module_from_spec(spec)
        sys.modules[cls.name] = module
        spec.loader.exec_module(module)
        cls.desktop = importlib.import_module(cls.name + ".desktop")
        cls.macos = importlib.import_module(cls.name + ".desktop_macos")
        cls.runtime = importlib.import_module(cls.name + ".runtime")

    @classmethod
    def tearDownClass(cls):
        for name in list(sys.modules):
            if name.startswith(cls.name):
                sys.modules.pop(name)

    def test_brand_formats_reuse_existing_artwork(self):
        icons = ROOT / "web/icons"
        png = (ROOT / "windows/app.png").read_bytes()
        self.assertEqual((icons / "fitsviewer.png").read_bytes(), png)
        self.assertEqual(struct.unpack(">II", png[16:24]), (256, 256))
        icns = (icons / "fitsviewer.icns").read_bytes()
        self.assertEqual(icns[:4], b"icns")
        self.assertEqual(struct.unpack(">I", icns[4:8])[0], len(icns))
        self.assertEqual(icns[8:12], b"ic08")
        self.assertEqual(icns[16:], png)
        ico = (ROOT / "windows/app.ico").read_bytes()
        self.assertEqual(struct.unpack("<HH", ico[:4]), (0, 1))
        sizes = {ico[6 + 16 * index] or 256 for index in range(struct.unpack("<H", ico[4:6])[0])}
        self.assertTrue({16, 24, 32, 48, 64, 128, 256}.issubset(sizes))
        svg = (icons / "fitsviewer.svg").read_text(encoding="utf-8").strip()
        self.assertEqual(ET.fromstring(svg).attrib["viewBox"], "0 0 256 256")
        self.assertIn("svgstr: `" + svg + "`", (ROOT / "jupyter/src/icon.ts").read_text(encoding="utf-8"))
        self.assertLess(sum(path.stat().st_size for path in icons.iterdir()), 8192)

    def test_jupyter_reuses_an_already_locked_shared_icon_module(self):
        package = json.loads((ROOT / "jupyter/package.json").read_text())
        lock = json.loads((ROOT / "jupyter/package-lock.json").read_text())
        name = "@jupyterlab/ui-components"
        self.assertEqual(package["dependencies"][name], lock["packages"][""]["dependencies"][name])
        self.assertIn("node_modules/" + name, lock["packages"])
        self.assertFalse(package["jupyterlab"]["sharedPackages"][name]["bundled"])
        source = (ROOT / "jupyter/src/index.ts").read_text(encoding="utf-8")
        self.assertIn("widget.title.icon = fitsviewerIcon", source)
        self.assertEqual(source.count("icon: fitsviewerIcon") + source.count("icon:fitsviewerIcon"), 1)
        self.assertIn("optional: [ILauncher]", source)
        self.assertIn("launcher?.add({command: openViewer, category: 'Other'})", source)
        name = "@jupyterlab/launcher"
        self.assertEqual(package["dependencies"][name], lock["packages"][""]["dependencies"][name])
        self.assertIn("node_modules/" + name, lock["packages"])
        self.assertFalse(package["jupyterlab"]["sharedPackages"][name]["bundled"])
        self.assertIn("defaultFor: preferences.default_viewer ? ['fitsviewer-fits'] : []", source)

    def test_jupyter_launcher_reuses_packaged_icon_without_starting_viewer(self):
        # Exercise only this configuration function; optional Jupyter imports stay out.
        # 只执行配置函数，不为图标检查导入可选 Jupyter 或启动查看器。
        tree = ast.parse((ROOT / "jupyter/fitsviewer/server.py").read_text(encoding="utf-8"))
        setup = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == "setup_server")
        def no_launch(*args):
            raise AssertionError("Launcher configuration must not start a process")
        namespace = {"resources": lambda: (ROOT, ROOT / "bin/fitsviewer", ROOT / "web"),
                     "_command": no_launch, "process_environment": no_launch,
                     "load_settings": lambda: {"proxy_port": 0}}
        exec(compile(ast.Module(body=[setup], type_ignores=[]), "setup_server", "exec"), namespace)
        config = namespace["setup_server"]()
        self.assertFalse(config["launcher_entry"]["enabled"])
        icon = Path(config["launcher_entry"]["icon_path"])
        self.assertEqual(icon.read_bytes(), (ROOT / "web/icons/fitsviewer.svg").read_bytes())
        self.assertIs(config["command"], no_launch)

    def test_linux_registration_copies_brand_and_declares_matching_window_class(self):
        with tempfile.TemporaryDirectory(prefix="icon-desktop-", dir=ROOT / "qa") as directory:
            scratch = Path(directory)
            self.assertTrue(scratch.resolve().is_relative_to((ROOT / "qa").resolve()))
            cli = scratch / "bin/fitsviewer"
            cli.parent.mkdir()
            cli.write_text("# fixture only\n")
            environment = {"XDG_DATA_HOME": str(scratch / "data"), "XDG_CONFIG_HOME": str(scratch / "config")}
            with patch.object(self.desktop, "os", SimpleNamespace(name="posix", environ=environment)), patch.object(self.desktop, "sys", SimpleNamespace(platform="linux", executable=str(cli.parent / "python"), prefix=str(scratch))), patch.object(self.runtime, "resources", return_value=(ROOT, cli, ROOT / "web")), patch.object(self.desktop.shutil, "which", return_value="fixture-tool"), patch.object(self.desktop.subprocess, "run", return_value=SimpleNamespace(stdout="")):
                self.desktop.install(default=False)
                paths = self.desktop._paths()
                self.assertEqual(paths["icon"].read_bytes(), (ROOT / "web/icons/fitsviewer.svg").read_bytes())
                self.assertIn("StartupWMClass=fitsviewer\n", paths["desktop"].read_text())
                self.assertIn("Icon=fitsviewer\n", paths["desktop"].read_text())
            native = (ROOT / "crates/server/src/main.rs").read_text(encoding="utf-8")
            self.assertIn('.with_app_id("fitsviewer")', native)
            self.assertIn('if cfg!(target_os="linux") { "fitsviewer" }', native)

    def test_all_profiles_stage_the_same_small_icons_in_the_manifest(self):
        with tempfile.TemporaryDirectory(prefix="icon-stage-", dir=ROOT / "qa") as directory:
            scratch = Path(directory)
            self.assertTrue(scratch.resolve().is_relative_to((ROOT / "qa").resolve()))
            shutil.copytree(ROOT / "web/icons", scratch / "web/icons")
            binary = scratch / "target/release" / ("fitsviewer.exe" if os.name == "nt" else "fitsviewer")
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"test fixture, never executed")
            for profile in ["mini", "desktop-mini", "remote-pro", "full"]:
                selection = profiles.select(profile)
                features = {"base": True, "desktop": "desktop" in selection.modules, "pro": "pro" in selection.modules}
                with self.subTest(profile=profile), patch.object(profiles, "ROOT", scratch), patch.object(profiles, "python_files", return_value=set()), patch.object(profiles.subprocess, "check_output", return_value=json.dumps({"features": features})), patch.object(profiles, "run"):
                    stage = profiles.stage(selection, scratch / "target")
                    manifest = json.loads((stage / "artifact-manifest.json").read_text())
                    for suffix in ["png", "svg", "icns"]:
                        name = "web/icons/fitsviewer." + suffix
                        self.assertIn(name, manifest["files"])
                        self.assertEqual((stage / name).read_bytes(), (ROOT / name).read_bytes())

    def test_macos_diagnostics_never_modify_existing_app_bundle(self):
        with tempfile.TemporaryDirectory(prefix="icon-macos-", dir=ROOT / "qa") as directory:
            scratch = Path(directory)
            self.assertTrue(scratch.resolve().is_relative_to((ROOT / "qa").resolve()))
            bundle = scratch / "FitsViewer.app"
            resources = bundle / "Contents/Resources"
            resources.mkdir(parents=True)
            info = bundle / "Contents/Info.plist"
            info.write_bytes(plistlib.dumps({"CFBundleIconFile": "FitsViewer"}))
            missing = self.macos.icon_details(bundle)
            self.assertFalse(missing["present"])
            self.assertIn("Contents/Resources/FitsViewer.icns", missing["repair_hint"])
            shutil.copy2(ROOT / "web/icons/fitsviewer.icns", resources / "FitsViewer.icns")
            before = {path: path.read_bytes() for path in bundle.rglob("*") if path.is_file()}
            self.assertTrue(self.macos.icon_details(bundle)["present"])
            self.assertEqual(before, {path: path.read_bytes() for path in bundle.rglob("*") if path.is_file()})
            info.write_bytes(plistlib.dumps({"CFBundleIconName": "AppIcon"}))
            (resources / "Assets.car").write_bytes(b"asset catalog presence fixture")
            self.assertTrue(self.macos.icon_details(bundle)["present"])


if __name__ == "__main__":
    unittest.main(verbosity=2)
