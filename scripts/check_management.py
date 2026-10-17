"""Exercise management commands without touching existing user servers."""
import json
import os
from pathlib import Path
import shlex
import signal
import shutil
import socket
import subprocess
import time
import urllib.request
from urllib.parse import urlsplit


def check_management(venv, environment, data):
    cli = str(venv / "bin/fitsview")
    def run(*arguments, check=True):
        return subprocess.run([cli, *arguments], env=environment, text=True, capture_output=True, check=check)
    run("ports", "set", "--jupyter", "auto", "--browser", "19182", "--viewer", "auto",
        "--proxy", "auto", "--ssh-port", "2222", "--ssh-host", "user@test-host")
    before = json.loads(run("ports", "--json").stdout)
    path = Path(before["config"])
    contents = path.read_bytes()
    assert run("ports", "set", "--browser", "70000", check=False).returncode != 0
    assert path.read_bytes() == contents
    tunnel = shlex.split(run("tunnel", "--remote-port", "8888").stdout.splitlines()[0])
    assert tunnel[tunnel.index("-L") + 1] == "127.0.0.1:19182:127.0.0.1:8888"
    assert tunnel[tunnel.index("-p") + 1] == "2222"
    assert tunnel[-1] == "user@test-host"
    assert before["jupyter"] is None and before["viewer"] is None
    conda = os.environ.get("CONDA_EXE") or shutil.which("conda")
    checked_conda = False
    if conda and Path(conda).is_file():
        info = json.loads(run("install", "--conda-env", "base", "--conda-exe", conda, "--check").stdout)
        assert info["supported_jupyterlab"]
        checked_conda = True
    server = None
    try:
        started = run("jupyter", "start", "--port", "auto", "--browser-port", "same", "--root", str(data))
        server = json.loads(run("status", "--json").stdout)["jupyter"]
        assert server and server["port"] > 0
        login = urlsplit(run("jupyter", "url").stdout.strip())
        assert login.port == server["port"]
        run("jupyter", "start", "--port", str(server["port"]), "--root", str(data))
        assert json.loads(run("status", "--json").stdout)["jupyter"]["pid"] == server["pid"]
        with socket.socket() as busy:
            busy.bind(("127.0.0.1", 0)); busy.listen()
            result = run("jupyter", "start", "--port", str(busy.getsockname()[1]), "--root", str(data), check=False)
            assert result.returncode != 0 and "busy" in result.stderr
        assert json.loads(run("status", "--json").stdout)["jupyter"]["pid"] == server["pid"]
    finally:
        if server:
            command = Path(f"/proc/{server['pid']}/cmdline")
            if command.exists() and str(venv).encode() in command.read_bytes():
                os.kill(server["pid"], signal.SIGTERM)
                for _ in range(100):
                    if not command.exists() or not command.read_bytes(): break
                    time.sleep(0.1)
                else: os.kill(server["pid"], signal.SIGKILL)
    return {"persistent_port_settings": True, "invalid_port_keeps_config": True,
            "tunnel_generation": True, "managed_jupyter_start_reuse": True,
            "busy_port_keeps_running_server": True, "conda_target_check": checked_conda}


def check_standalone(venv, environment, image):
    cli = str(venv / "bin/fitsview")
    subprocess.run([cli, "ports", "set", "--viewer", "auto"], env=environment, check=True, capture_output=True)
    state = None
    try:
        subprocess.run([cli, str(image), "--no-browser"], env=environment, check=True, capture_output=True)
        snapshot = json.loads(subprocess.check_output([cli, "status", "--json"], env=environment, text=True))
        state = snapshot["viewer"]
        assert state["running"] and state["port"] > 0
    finally:
        if state:
            executable = Path(f"/proc/{state['pid']}/exe")
            if executable.exists() and venv in executable.resolve().parents:
                os.kill(state["pid"], signal.SIGTERM)
    return True
