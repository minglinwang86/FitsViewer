"""Open the real Rust GUI on an isolated X display and capture it."""
from pathlib import Path
import json
import os
import select
import shutil
import subprocess
import time
from PIL import ImageGrab, ImageStat

root = Path(__file__).resolve().parents[1]
for directory in [".runtime", "logs", "reports/screenshots"]:
    (root / directory).mkdir(parents=True, exist_ok=True)
children = []
try:
    xvfb = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1280x900x24", "-nolisten", "tcp"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    children.append(xvfb)
    if not select.select([xvfb.stdout], [], [], 15)[0]:
        raise TimeoutError("Xvfb did not start")
    number = xvfb.stdout.readline().decode().strip()
    assert number.isdigit()
    display = ":" + number
    env = os.environ.copy()
    env["DISPLAY"] = display
    env["LIBGL_ALWAYS_SOFTWARE"] = "1"
    runtime = root / ".runtime/native-xdg"
    runtime.mkdir(parents=True, exist_ok=True)
    runtime.chmod(0o700)
    env["XDG_RUNTIME_DIR"] = str(runtime)
    with (root / "logs/native.log").open("wb") as log:
        app = subprocess.Popen([str(root / "target/release/fitsview"), "--native", str(root / "examples/starfield.fits")], env=env, stdout=log, stderr=subprocess.STDOUT)
    children.append(app)
    time.sleep(5)
    assert app.poll() is None, (root / "logs/native.log").read_text()
    screenshot = ImageGrab.grab(xdisplay=display)
    assert max(ImageStat.Stat(screenshot).var) > 100
    shots = root / "reports/screenshots"
    shots.mkdir(exist_ok=True)
    screenshot.save(shots / "native_gui.png")
    windows = ""
    if shutil.which("xwininfo"):
        windows = subprocess.check_output(["xwininfo", "-root", "-tree"], env=env, text=True)
        assert "FITSView" in windows
    report = {"passed": True, "native_window_alive": True, "screenshot_nonblank": True, "display": "isolated Xvfb", "forwarding_tested": False, "window_tree": windows}
    (root / "reports/native_validation.json").write_text(json.dumps(report, indent=2))
    print(json.dumps({"passed": True, "native_window": True}))
finally:
    for child in reversed(children):
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=5)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
