"""Verify the relocated native macOS Full portable; keep GUI claims separate.

验收解压后 macOS 原生产物；CLI/数值/进程验收与 GUI 交互结论分开记录。
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import plistlib
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

from build_macos_full import ROOT, digest, lock

REAL_FITS_URL = "https://data.astropy.org/tutorials/FITS-images/HorseHead.fits"


def execute(arguments, environment, *, binary=False, input_text=None):
    result = subprocess.run(list(map(str, arguments)), env=environment, input=input_text,
                            capture_output=True, text=not binary, timeout=30)
    if result.returncode:
        raise RuntimeError(str(arguments[0]) + ": " + str(result.stderr)[-2500:])
    return result.stdout


def read_packet(packet):
    count = int.from_bytes(packet[:4], "little")
    if count <= 0 or count > 1024 * 1024:
        raise AssertionError("Invalid dataset packet header")
    return json.loads(packet[4:4 + count]), packet[4 + count:]


def process_alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--probe-gui", action="store_true", help="Try startup only; no interactive acceptance")
    args = parser.parse_args()
    if sys.platform != "darwin":
        raise RuntimeError("This acceptance script must run on macOS")
    import numpy as np
    from astropy.io import fits

    package_dir = args.package.resolve()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    package = json.loads((package_dir / "package.json").read_text())
    report = {"schema": 1, "passed": False, "source_commit": package["source_commit"],
              "architecture": package["architecture"], "checks": {},
              "gui": {"interactive_verified": False, "startup_probe": "not requested"},
              "limitations": ["No interactive native GUI acceptance or Finder file-association test",
                              "No Developer ID signature, notarization or quarantine/Gatekeeper acceptance",
                              "No older-macOS execution test; CI runs on macOS 15",
                              "Real image loading is not a scientific validation of every PRO tool",
                              "Prebuilt Lab assets are checked; no macOS Python wheel/Jupyter session test"]}
    report_path = output / "validation.json"

    def save():
        report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    save()
    try:
        archive = package_dir / package["portable"]
        assert digest(archive) == package["portable_sha256"]
        with tempfile.TemporaryDirectory(prefix="fitsviewer-macos-") as temporary:
            work = Path(temporary)
            relocated = work / "解压目录 空格"
            relocated.mkdir()
            environment = dict(os.environ, FITSVIEWER_RUNTIME_DIR=str(work / "runtime"))
            execute(["ditto", "-x", "-k", archive, relocated], environment)
            candidates = list(relocated.glob("*/FitsViewer.app"))
            assert len(candidates) == 1
            app = candidates[0]
            resources = app / "Contents/Resources"
            binary = resources / "bin/fitsviewer"
            launcher = app / "Contents/MacOS/FitsViewer"
            wrapper = app.parent / "fitsviewer"
            assert os.access(binary, os.X_OK) and os.access(launcher, os.X_OK) and os.access(wrapper, os.X_OK)
            assert digest(binary) == package["native_binary_sha256"]
            assert digest(launcher) == package["native_launcher_sha256"]
            assert digest(resources / "web/pkg/fitsviewer_web_bg.wasm") == lock()["wasm_sha256"]
            assert [p.name for p in (resources / "bin").iterdir()] == ["fitsviewer"]
            assert not any(p.suffix.lower() in {".fits", ".fit", ".fz", ".exe", ".whl"}
                           for p in app.rglob("*") if p.is_file())
            with (app / "Contents/Info.plist").open("rb") as stream:
                info = plistlib.load(stream)
            assert info["CFBundleIconFile"] == "FitsViewer.icns"
            assert (resources / info["CFBundleIconFile"]).read_bytes()[:4] == b"icns"
            assert "CFBundleTypeIconFile" not in json.dumps(info)
            execute(["codesign", "--verify", "--deep", "--strict", "--verbose=2", app], environment)
            dependencies = execute(["otool", "-L", binary], environment)
            for dependency in dependencies.splitlines()[1:]:
                assert dependency.strip().startswith(("/usr/lib/", "/System/Library/")), dependency
            (output / "native-dependencies.txt").write_text(dependencies)
            report["checks"]["archive_relocation_signature_icon"] = True
            expected_features = {"base": True, "pro": True, "desktop": True}
            for entry in [binary, launcher, wrapper]:
                build_info = json.loads(execute([entry, "--build-info"], environment))
                assert build_info["features"] == expected_features
                assert build_info["version"] == package["version"]
            report["checks"]["native_loader_and_full_entrypoints"] = True
            save()

            real_file = work / "真实图像 空格.fits"
            with urllib.request.urlopen(REAL_FITS_URL, timeout=60) as response:
                data = response.read(8 * 1024 * 1024 + 1)
            assert len(data) < 8 * 1024 * 1024 and data.startswith(b"SIMPLE  =")
            real_file.write_bytes(data)
            report["real_data"] = {"source": REAL_FITS_URL, "sha256": digest(real_file),
                                   "bytes": real_file.stat().st_size, "included_in_package": False}
            inspected = json.loads(execute([wrapper, real_file, "--inspect-json"], environment))
            info_packet, samples = read_packet(execute([wrapper, real_file, "--packet"], environment, binary=True))
            with fits.open(real_file, do_not_scale_image_data=True, uint=False) as hdus:
                reference_raw = np.array(hdus[0].data)
            assert reference_raw.shape == (893, 891)
            dtype = {8: "u1", 16: "<i2", 32: "<i4", 64: "<i8", -32: "<f4", -64: "<f8"}[info_packet["bitpix"]]
            raw = np.frombuffer(samples, dtype=dtype).reshape(info_packet["height"], info_packet["width"])
            np.testing.assert_array_equal(raw, reference_raw)
            with fits.open(real_file, uint=False) as hdus:
                physical = np.asarray(hdus[0].data, dtype="float64")
            actual_physical = raw.astype("float64") * info_packet["bscale"] + info_packet["bzero"]
            if info_packet["blank"] is not None:
                actual_physical[raw == info_packet["blank"]] = np.nan
            np.testing.assert_allclose(actual_physical, physical, rtol=2e-7, equal_nan=True)
            stats = inspected["statistics"]
            assert stats["valid"] == int(np.isfinite(physical).sum())
            np.testing.assert_allclose([stats["minimum"], stats["maximum"]], [np.nanmin(physical), np.nanmax(physical)])
            header = json.loads(execute([wrapper, real_file, "--header-json"], environment))
            assert header["merged"] is False and "SIMPLE" in header["header"]
            view = json.loads(execute([wrapper, "--validate-view"], environment, input_text='{"schema":1}'))
            assert view["schema"] == 1
            report["checks"]["real_fits_raw_and_physical_values_vs_astropy"] = True
            save()

            server_environment = dict(environment, FITSVIEWER_HOME=str(resources))
            runtime_path = work / "runtime/server.json"
            with (output / "server.log").open("wb") as log:
                server = subprocess.Popen([str(wrapper), "--serve", "--port", "0", "--no-browser",
                                           "--root", str(work),
                                           "--python", sys.executable], env=server_environment,
                                          stdout=log, stderr=subprocess.STDOUT)
            worker_pid = None
            try:
                deadline = time.monotonic() + 20
                while not runtime_path.is_file():
                    if server.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError("Packaged server did not become ready")
                    time.sleep(0.1)
                runtime = json.loads(runtime_path.read_text())
                assert runtime["pid"] == server.pid
                base = "http://127.0.0.1:" + str(runtime["port"])

                def request(path, payload=None, authenticated=True):
                    headers = {"X-Fitsviewer-Token": runtime["token"]} if authenticated else {}
                    body = None
                    if payload is not None:
                        headers["Content-Type"] = "application/json"
                        body = json.dumps(payload).encode()
                    with urllib.request.urlopen(urllib.request.Request(base + path, data=body, headers=headers), timeout=30) as reply:
                        return reply.read()

                health = json.loads(request("/api/health"))
                assert health["features"] == expected_features
                assert request("/pkg/fitsviewer_web_bg.wasm") == (resources / "web/pkg/fitsviewer_web_bg.wasm").read_bytes()
                query = "/api/image?" + urllib.parse.urlencode({"path": str(real_file)})
                try:
                    request(query, authenticated=False)
                    raise AssertionError("Unauthenticated data request was accepted")
                except urllib.error.HTTPError as error:
                    assert error.code == 401
                assert request(query) == execute([binary, real_file, "--packet"], environment, binary=True)
                capabilities = json.loads(request("/api/science", {"operation": "capabilities", "params": {}}))
                assert capabilities["bundled_scientific_packages"] is False
                assert capabilities["loaded_modules"] == []
                assert Path(capabilities["python"]).resolve() == Path(sys.executable).resolve()
                assert "spectral_analysis" in capabilities["operations"]
                status = json.loads(request("/api/science", {"operation": "runtime_status", "params": {}}))
                worker_pid = status["worker_pid"]
                assert status["worker_running"] and worker_pid and process_alive(worker_pid)
                report["checks"]["loopback_api_auth_packet_and_external_lazy_worker"] = True
                save()
                server.send_signal(signal.SIGTERM)
                assert server.wait(timeout=12) == 0
                deadline = time.monotonic() + 5
                while process_alive(worker_pid) and time.monotonic() < deadline:
                    time.sleep(0.1)
                assert not process_alive(worker_pid), "Scientific worker survived host shutdown"
                report["checks"]["sigterm_reaps_external_scientific_worker"] = True
            finally:
                if server.poll() is None:
                    server.terminate()
                    try:
                        server.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        server.kill()
                        server.wait()

            if args.probe_gui:
                with (output / "native-startup.log").open("wb") as log:
                    gui = subprocess.Popen([str(wrapper), str(real_file), "--python", sys.executable],
                                           env=environment, stdout=log, stderr=subprocess.STDOUT)
                try:
                    time.sleep(5)
                    report["gui"]["startup_probe"] = "alive after 5s" if gui.poll() is None else "exited"
                    report["gui"]["startup_returncode"] = gui.poll()
                    if gui.poll() is None:
                        captured = subprocess.run(["screencapture", "-x", str(output / "native-startup.png")],
                                                  capture_output=True, timeout=10)
                        report["gui"]["screenshot_captured"] = captured.returncode == 0
                finally:
                    if gui.poll() is None:
                        gui.terminate()
                        try:
                            gui.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            gui.kill()
                            gui.wait()
            report["passed"] = True
            save()
            print(json.dumps({"passed": True, "checks": list(report["checks"]), "interactive_gui_verified": False}))
    except Exception as error:
        report["error"] = str(error)
        save()
        raise


if __name__ == "__main__":
    main()
