"""Stdlib-only architecture checks. No Cargo, npm, wheel build, or service launch.

仅检查模块选择、拒绝边界与默认不执行约定，不触发编译或打包。
"""
from contextlib import redirect_stdout
from pathlib import Path
from unittest.mock import patch
import hashlib
import importlib.util
import io
import json
import sys
import tempfile
import tomllib
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import build_profiles as profiles
import build_wheel as wheel


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ModuleSelection(unittest.TestCase):
    def test_four_profiles_have_expected_native_and_pro_dependencies(self):
        for name, native, pro in [("mini", False, False), ("desktop-mini", True, False),
                                  ("remote-pro", False, True), ("full", True, True)]:
            with self.subTest(name=name):
                selection = profiles.select(name)
                plan = profiles.plan(selection)
                self.assertEqual(selection.host_features, (["desktop"] if native else []) + (["pro"] if pro else []))
                self.assertEqual(selection.viewer_features, ["pro"] if pro else [])
                self.assertFalse(plan["source_rewriting"])
                self.assertIn("fitsviewer_gaia.py", plan["python_adapters"])
                self.assertEqual("fitsviewer_spectral_analysis.py" in plan["python_adapters"], pro)

    def test_default_and_plan_never_spawn_or_stage(self):
        for arguments in [[], ["--profile", "mini"], ["--profile", "full", "--wheel", "--plan"]]:
            with self.subTest(arguments=arguments), patch.object(profiles, "run") as run, patch.object(profiles, "stage") as stage:
                output = io.StringIO()
                with redirect_stdout(output):
                    self.assertEqual(profiles.main(arguments), 0)
                self.assertFalse(json.loads(output.getvalue())["will_execute"])
                run.assert_not_called()
                stage.assert_not_called()

    def test_custom_base_and_closure(self):
        self.assertEqual(profiles.select(modules=[]).modules, ("base",))
        self.assertEqual(profiles.select(modules=["pro"]).modules, ("base", "pro"))

    def test_unknown_and_ambiguous_selection_rejected(self):
        for call in [lambda: profiles.select("unknown"), lambda: profiles.select(modules=["typo"]),
                     lambda: profiles.select("mini", ["pro"])]:
            with self.assertRaises(ValueError):
                call()

    def test_no_jupyter_plan_has_no_npm(self):
        commands = profiles.plan(profiles.select(modules=["base"]))["commands"]
        self.assertFalse(any(c["argv"][0] == "npm" for c in commands))

    def test_omitted_jupyter_removes_both_registration_tables(self):
        text = (ROOT / "jupyter/pyproject.toml").read_text(encoding="utf-8")
        for table in ["project.entry-points.jupyter_serverproxy_servers", "tool.hatch.build.targets.wheel.shared-data"]:
            text = wheel.remove_table(text, table)
        parsed = tomllib.loads(text)
        self.assertNotIn("entry-points", parsed["project"])
        self.assertNotIn("shared-data", parsed["tool"]["hatch"]["build"]["targets"]["wheel"])
        self.assertIn("optional-dependencies", parsed["project"])

    def test_staged_manifest_rejects_tampering_and_profile_path(self):
        with tempfile.TemporaryDirectory(prefix="profile-check-", dir=ROOT / "qa") as directory:
            root = Path(directory)
            self.assertTrue(root.resolve().is_relative_to((ROOT / "qa").resolve()))
            metadata = profiles.select("mini").metadata()
            for name in ["build-profile.json", "web/build-profile.json"]:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps(metadata), encoding="utf-8")
            for name in profiles.python_files(profiles.select("mini")):
                path = root / "web/python" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("# inventory fixture only\n", encoding="utf-8")
            files = {p.relative_to(root).as_posix(): {"bytes": p.stat().st_size, "sha256": hashlib.sha256(p.read_bytes()).hexdigest()} for p in root.rglob("*") if p.is_file()}
            (root / "artifact-manifest.json").write_text(json.dumps({"schema": 1, "features": metadata, "files": files}), encoding="utf-8")
            self.assertEqual(wheel.validate_assets(root), metadata)
            path.write_text("changed", encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "changed"):
                wheel.validate_assets(root)
            metadata["profile"] = "../../escape"
            (root / "build-profile.json").write_text(json.dumps(metadata), encoding="utf-8")
            with self.assertRaisesRegex(ValueError, "profile"):
                wheel.validate_assets(root)


class AbsentProBoundary(unittest.TestCase):
    def test_mini_denies_pro_before_loading_external_adapters(self):
        module = load("profile_guard_test", ROOT / "web/python/fitsviewer_build_profile.py")
        with patch.object(module, "profile", return_value={"pro": False}):
            for operation in ["spectral_analysis", "image_flux_match", "detect_stars", "rgb", "vo_query"]:
                with self.subTest(operation=operation), self.assertRaisesRegex(ValueError, "PRO"):
                    module.require_operation(operation, {})
            with self.assertRaisesRegex(ValueError, "Gaia"):
                module.require_operation("job_start", {"operation": "spectral_analysis"})
            module.require_operation("job_start", {"operation": "gaia_solve"})
            module.require_operation("wcs", {})

    def test_importing_python_api_does_not_import_jupyter(self):
        before = set(sys.modules)
        spec = importlib.util.spec_from_file_location("profile_api_import_check", ROOT / "jupyter/fitsviewer/__init__.py", submodule_search_locations=[str(ROOT / "jupyter/fitsviewer")])
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        try:
            spec.loader.exec_module(module)
            self.assertTrue(module.build_info()["jupyter"])
        finally:
            for name in list(sys.modules):
                if name.startswith(spec.name): del sys.modules[name]
        self.assertFalse(any(name.startswith(("jupyter", "IPython", "astropy")) for name in set(sys.modules) - before))


if __name__ == "__main__":
    unittest.main(verbosity=2)
