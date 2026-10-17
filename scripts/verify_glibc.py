"""Run the packaged server with archived Debian glibc libraries, without Docker.

在独立目录使用 Debian 旧版 libc 加载器验证 wheel 后端；不替换系统库。
"""
from pathlib import Path
import argparse
import hashlib
import json
import lzma
import os
import subprocess
import tempfile
import time
import urllib.request
import zipfile

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("wheel", type=Path)
args = parser.parse_args()
results = []
for distro, label, packages in [("buster", "2.28", ["libc6", "libgcc1"]), ("bullseye", "2.31", ["libc6", "libgcc-s1"])]:
    destination = root / ".tools/compat" / distro
    destination.mkdir(parents=True, exist_ok=True)
    index = destination / "Packages.xz"
    base = "https://archive.debian.org/debian/"
    if not index.exists(): urllib.request.urlretrieve(base + "dists/" + distro + "/main/binary-amd64/Packages.xz", index)
    records = {}
    with lzma.open(index, "rt") as data:
        for paragraph in data.read().split("\n\n"):
            if not any(paragraph.startswith("Package: " + name + "\n") for name in packages): continue
            record = dict(line.split(": ", 1) for line in paragraph.splitlines() if ": " in line and not line.startswith(" "))
            records[record["Package"]] = record
    for name in packages:
        record = records[name]
        package = destination / Path(record["Filename"]).name
        if not package.exists(): urllib.request.urlretrieve(base + record["Filename"], package)
        assert hashlib.sha256(package.read_bytes()).hexdigest() == record["SHA256"]
        subprocess.run(["dpkg-deb", "--extract", str(package), str(destination)], check=True)
    loader = destination / ("lib/x86_64-linux-gnu/ld-" + label + ".so")
    libraries = ":".join(str(destination / d) for d in ["lib/x86_64-linux-gnu", "usr/lib/x86_64-linux-gnu"])
    with tempfile.TemporaryDirectory(prefix="fitsviewer-glibc-") as temporary:
        scratch = Path(temporary)
        with zipfile.ZipFile(args.wheel) as archive:
            binary = scratch / "fitsviewer"
            binary.write_bytes(archive.read("fitsviewer/_vendor/bin/fitsviewer")); binary.chmod(0o755)
            for name in archive.namelist():
                prefix = "fitsviewer/_vendor/web/"
                if name.startswith(prefix) and not name.endswith("/"):
                    target = scratch / "web" / name.removeprefix(prefix)
                    target.parent.mkdir(parents=True, exist_ok=True); target.write_bytes(archive.read(name))
        # Relocate the test loader beside the packaged resources, away from source checkouts.
        # 将测试加载器移到包资源旁，避免其路径误指向原源码目录。
        relocated_loader = scratch / "ld-compat.so"
        relocated_loader.write_bytes(loader.read_bytes()); relocated_loader.chmod(0o755)
        command = [str(relocated_loader), "--library-path", libraries, str(binary)]
        version = subprocess.check_output([*command, "--version"], text=True).strip()
        env = {k: v for k, v in os.environ.items() if not k.startswith("FITSVIEWER_")}
        env.update(FITSVIEWER_RUNTIME_DIR=str(scratch / "state"), LC_ALL="C")
        process = None
        try:
            with (scratch / "server.log").open("w") as log:
                process = subprocess.Popen([*command, "--serve", "--port", "0", "--root", str(root / "examples")], cwd=scratch, env=env, stdout=log, stderr=subprocess.STDOUT)
            state = scratch / "state/server.json"
            for _ in range(100):
                if state.exists(): break
                if process.poll() is not None: raise RuntimeError((scratch / "server.log").read_text())
                time.sleep(.1)
            info = json.loads(state.read_text())
            request = urllib.request.Request(f"http://127.0.0.1:{info['port']}/api/image?path=starfield.fits", headers={"X-Fitsviewer-Token": info["token"]})
            with urllib.request.urlopen(request) as response: packet = response.read()
            with urllib.request.urlopen(f"http://127.0.0.1:{info['port']}/") as response:
                assert b'fitsviewer-canvas' in response.read(), 'Runtime resource discovery failed'
            length = int.from_bytes(packet[:4], "little"); image = json.loads(packet[4:4+length])
            assert image["width"] == 512 and image["height"] == 512
            results.append({"debian_library_set": distro, "glibc": label, "version": version, "fits_api": True,
                            "resource_discovery_without_build_path": True,
                            "full_distribution_test": False, "native_desktop_test": False})
        finally:
            if process and process.poll() is None:
                process.terminate()
                try: process.wait(timeout=10)
                except subprocess.TimeoutExpired: process.kill(); process.wait()
report = {"wheel": args.wheel.name, "sha256": hashlib.sha256(args.wheel.read_bytes()).hexdigest(), "passed": True, "loader_smoke_tests": results}
(root / "reports").mkdir(exist_ok=True)
(root / "reports/glibc_validation.json").write_text(json.dumps(report, indent=2))
print(json.dumps(report))
