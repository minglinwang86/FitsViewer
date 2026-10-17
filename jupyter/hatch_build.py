"""Reject incomplete wheels and mark the bundled executable as platform-specific."""
from pathlib import Path
import platform
import os
from hatchling.builders.hooks.plugin.interface import BuildHookInterface


class CustomBuildHook(BuildHookInterface):
    def initialize(self, version, build_data):
        if self.target_name != "wheel":
            return
        root = Path(self.root)
        windows = os.environ.get('FITSVIEWER_PACKAGE_TARGET')=='windows'
        executable = 'fitsviewer.exe' if windows else 'fitsviewer'
        for name in ["fitsviewer/_vendor/bin/"+executable,
                     "fitsviewer/_vendor/web/pkg/fitsviewer_web_bg.wasm",
                     "fitsviewer/labextension/package.json"]:
            if not (root / name).is_file():
                raise RuntimeError("Missing prebuilt assets. Run scripts/build.sh before packaging.")
        binary = (root / ('fitsviewer/_vendor/bin/'+executable)).read_bytes()
        if windows:
            header=int.from_bytes(binary[0x3c:0x40],'little')
            if binary[:2]!=b'MZ' or binary[header:header+4]!=b'PE\0\0' or int.from_bytes(binary[header+4:header+6],'little')!=0x8664:
                raise RuntimeError('Bundled executable is not Windows x86_64.')
            build_data['pure_python']=False
            build_data['tag']='py3-none-win_amd64'
            return
        machine = platform.machine()
        if platform.system() != "Linux" or machine not in {"x86_64", "aarch64"}:
            raise RuntimeError("Build this wheel on Linux x86_64 or aarch64.")
        expected = {"x86_64": 62, "aarch64": 183}[machine]
        if binary[:4] != b"\x7fELF" or int.from_bytes(binary[18:20], "little") != expected:
            raise RuntimeError("Bundled executable does not match the wheel architecture.")
        build_data["pure_python"] = False
        build_data["tag"] = "py3-none-linux_" + machine
