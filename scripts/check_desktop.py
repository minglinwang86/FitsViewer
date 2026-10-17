"""Exercise user MIME registration and launch through a real desktop entry.

在隔离 XDG 目录验证默认关联，并通过 GIO 在独立 Xvfb 中启动原生窗口。
"""
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import time


def check_desktop(venv, environment, image):
    tools = ["xdg-mime", "update-mime-database", "desktop-file-validate", "gio", "Xvfb", "xwininfo"]
    if not all(shutil.which(tool) for tool in tools): return {"tested": False, "reason": "Optional desktop test tools unavailable"}
    env = dict(environment, PATH=str(venv / "bin") + os.pathsep + os.environ["PATH"])
    cli = str(venv / "bin/fitsview")
    apps = Path(env["XDG_DATA_HOME"]) / "applications"
    apps.mkdir(parents=True, exist_ok=True)
    prior = apps / "previous-fits-viewer.desktop"
    prior.write_text("[Desktop Entry]\nType=Application\nName=Previous FITS viewer\nExec=/usr/bin/true %f\nMimeType=image/fits;application/fits;application/x-fits-gzip;\n")
    for mime in ["image/fits", "application/fits", "application/x-fits-gzip"]:
        subprocess.run(["xdg-mime", "default", prior.name, mime], env=env, check=True)
    def run(*arguments):
        return subprocess.check_output([cli, *arguments], env=env, text=True)
    run("defaults", "set", "--desktop", "yes")
    desktop = apps / "fitsview.desktop"
    subprocess.run(["desktop-file-validate", str(desktop)], env=env, check=True)
    assert str(venv / "bin/fitsview") in desktop.read_text()
    assert "--native %f" in desktop.read_text()
    assert json.loads(run("desktop", "status"))["defaults"]["image/fits"] == desktop.name
    assert subprocess.check_output(["xdg-mime", "query", "filetype", str(image)], env=env, text=True).strip() == "image/fits"
    display = subprocess.Popen(["Xvfb", "-displayfd", "1", "-screen", "0", "1024x768x24", "-nolisten", "tcp"], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    try:
        assert select.select([display.stdout], [], [], 15)[0]
        number = display.stdout.readline().decode().strip()
        assert number.isdigit()
        native_env = dict(env, DISPLAY=":" + number, LIBGL_ALWAYS_SOFTWARE="1")
        # GIO's child inherits these streams and outlives the launcher.
        # GIO 子窗口会继承输出并长期运行，不能通过 PIPE 等待输出结束。
        subprocess.run(["gio", "launch", str(desktop), str(image)], env=native_env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15)
        for _ in range(100):
            windows = subprocess.check_output(["xwininfo", "-root", "-tree"], env=native_env, text=True)
            if "FITSView" in windows: break
            time.sleep(.1)
        assert "FITSView" in windows, "Desktop entry did not create the native window"
    finally:
        for proc in Path("/proc").iterdir():
            if not proc.name.isdigit(): continue
            try: executable = (proc / "exe").resolve(strict=True)
            except OSError: continue
            if venv in executable.parents and "_vendor/bin/fitsview" in str(executable): os.kill(int(proc.name), signal.SIGTERM)
        display.terminate()
        try: display.wait(timeout=5)
        except subprocess.TimeoutExpired: display.kill(); display.wait()
    run("defaults", "set", "--desktop", "no")
    assert json.loads(run("desktop", "status"))["defaults"]["image/fits"] == prior.name
    run("desktop", "remove")
    assert not desktop.exists()
    assert prior.exists()
    return {"tested": True, "desktop_entry_valid": True, "target_environment_with_spaces": True,
            "mime_default_enabled": True, "previous_default_restored": True,
            "gio_native_window": True, "isolated_display": "Xvfb", "x2go_tested": False}
