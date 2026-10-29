"""Smoke-check the newly packaged CI wheel; reuse accepted common Linux tests."""
from pathlib import Path, PurePosixPath
import argparse
import json
import os
import re
import signal
import subprocess
import tempfile
import time
import urllib.request
import urllib.parse
import zipfile

import build_macos_full as shared


def verify(wheel, lock_path, output):
    shared.LOCK_PATH = lock_path.resolve()
    expected = shared.lock()
    report = {"schema": 1, "source_commit": expected["source_commit"],
              "wheel": wheel.name, "sha256": shared.digest(wheel),
              "common_science_and_ui_tests": "reused accepted Linux evidence; not rerun"}
    with tempfile.TemporaryDirectory(prefix="fitsviewer-ci-package-") as temporary:
        work = Path(temporary)
        vendor = work / "relocated package"
        with zipfile.ZipFile(wheel) as archive:
            assert archive.testzip() is None
            for entry in archive.infolist():
                prefix = "fitsviewer/_vendor/"
                if entry.is_dir() or not entry.filename.startswith(prefix):
                    continue
                relative = PurePosixPath(entry.filename[len(prefix):])
                assert not relative.is_absolute() and ".." not in relative.parts
                target = vendor.joinpath(*relative.parts)
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(archive.read(entry))
        binary = vendor / "bin/fitsviewer"
        binary.chmod(0o755)
        assert shared.digest(vendor / "web/pkg/fitsviewer_web_bg.wasm") == expected["wasm_sha256"]
        environment = dict(os.environ, FITSVIEWER_RUNTIME_DIR=str(work / "runtime"),
                           FITSVIEWER_HOME=str(vendor), FITSVIEWER_CONFIG=str(work / "settings.json"))
        build_info = json.loads(subprocess.check_output([binary, "--build-info"], env=environment, text=True))
        assert build_info["version"] == expected["version"]
        assert build_info["features"] == {"base": True, "pro": True, "desktop": True}
        versions = subprocess.check_output(["readelf", "--version-info", binary], text=True)
        glibc = sorted({tuple(map(int, match.split('.'))) for match in re.findall(r"GLIBC_([\d.]+)", versions)})
        assert glibc and glibc[-1] <= (2, 28), glibc
        report.update(full_build_info=build_info, maximum_glibc=".".join(map(str, glibc[-1])),
                      shared_wasm_matches=True)
        data = work / "Unicode 星场 空格"
        data.mkdir()
        (data / "图像.fits").write_bytes((shared.ROOT / "examples/starfield.fits").read_bytes())
        with (work / "server.log").open("wb") as log:
            process = subprocess.Popen([binary, "--serve", "--port", "0", "--root", data],
                                       env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            runtime = work / "runtime/server.json"
            for _ in range(100):
                if runtime.is_file():
                    break
                if process.poll() is not None:
                    raise RuntimeError((work / "server.log").read_text())
                time.sleep(.1)
            session = json.loads(runtime.read_text())
            query = urllib.parse.urlencode({"path": "图像.fits"})
            request = urllib.request.Request(f"http://127.0.0.1:{session['port']}/api/image?" + query,
                                             headers={"X-Fitsviewer-Token": session["token"]})
            with urllib.request.urlopen(request, timeout=30) as response:
                packet = response.read()
            size = int.from_bytes(packet[:4], "little")
            info = json.loads(packet[4:4+size])
            assert (info["width"], info["height"]) == (512, 512), info
            report["relocated_unicode_authenticated_image"] = True
        finally:
            if process.poll() is None:
                process.send_signal(signal.SIGTERM)
                try:
                    process.wait(timeout=12)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
            assert process.returncode == 0, process.returncode
            report["server_exited"] = True
    report["passed"] = True
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", required=True, type=Path)
    parser.add_argument("--assets-lock", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    verify(args.wheel.resolve(), args.assets_lock, args.output)
