"""Reject incomplete wheels and mark the bundled executable as platform-specific."""
from pathlib import Path
import platform
from hatchling.builders.hooks.plugin.interface import BuildHookInterface


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        if self.target_name != "wheel":
            return
        root = Path(self.root)
        for name in ["fitsview_jupyter/_vendor/bin/fitsview",
                     "fitsview_jupyter/_vendor/web/pkg/fitsview_web_bg.wasm",
                     "fitsview_jupyter/labextension/package.json"]:
            if not (root / name).is_file():
                raise RuntimeError("Missing prebuilt assets. Run scripts/build.sh before packaging.")
        machine = platform.machine()
        if platform.system() != "Linux" or machine not in {"x86_64", "aarch64"}:
            raise RuntimeError("Build this wheel on Linux x86_64 or aarch64.")
        binary = (root / "fitsview_jupyter/_vendor/bin/fitsview").read_bytes()
        expected = {"x86_64": 62, "aarch64": 183}[machine]
        if binary[:4] != b"\x7fELF" or int.from_bytes(binary[18:20], "little") != expected:
            raise RuntimeError("Bundled executable does not match the wheel architecture.")
        build_data["pure_python"] = False
        build_data["tag"] = "py3-none-linux_" + machine
