"""Port inspection and managed Jupyter launch. / 端口检查、SSH 命令和 Jupyter 启动。"""
import argparse
import json
import os
from pathlib import Path
import shlex
import socket
import subprocess
import sys
import time
import urllib.request
from urllib.parse import urlencode, urlsplit, urlunsplit
from .runtime import resources, runtime_dir
from .settings import config_path, load_settings, port_value, save_settings


def jupyter_info():
    directory = runtime_dir()
    browser_port = None
    try:
        if (directory / "jupyter.json").is_file():
            link = json.loads((directory / "jupyter.json").read_text())
            pid = int(link["pid"])
            browser_port = link.get("browser_port")
            info = Path(link["runtime_dir"]) / f"jpserver-{pid}.json"
        elif (directory / "jupyter.pid").is_file():
            from jupyter_core.paths import jupyter_runtime_dir
            pid = int((directory / "jupyter.pid").read_text())
            info = Path(jupyter_runtime_dir()) / f"jpserver-{pid}.json"
        else: return None
        os.kill(pid, 0)
        server = json.loads(info.read_text())
        server["fitsview_browser_port"] = browser_port
        return server
    except (OSError, ValueError, KeyError): return None


def snapshot():
    settings = load_settings()
    jupyter = jupyter_info()
    viewer = None
    state = runtime_dir() / "server.json"
    if state.is_file():
        try:
            source = json.loads(state.read_text())
            viewer = {k: source[k] for k in ["port", "pid", "root", "public_url"]}
            address = urlsplit(viewer["public_url"])
            viewer["public_url"] = urlunsplit((address.scheme, address.netloc, address.path, "", ""))
            with urllib.request.urlopen(f"http://127.0.0.1:{viewer['port']}/api/health", timeout=1) as reply:
                viewer["running"] = json.load(reply).get("viewer") == "fitsview"
        except (OSError, ValueError, KeyError):
            if viewer: viewer["running"] = False
    active = None
    if jupyter:
        active = {k: jupyter.get(k) for k in ["port", "pid", "root_dir", "base_url"]}
        active["url"] = jupyter["url"].split("?", 1)[0]
    browser = None
    if viewer and viewer.get("running"):
        browser = urlsplit(viewer["public_url"]).port
    return {"python": sys.executable, "prefix": sys.prefix, "config": str(config_path()),
            "configured": settings, "jupyter": active, "viewer": viewer, "active_browser_port": browser}


def status(arguments):
    parser = argparse.ArgumentParser(prog="fitsview status", description="Show active services and configured ports; never starts a service.")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(arguments)
    data = snapshot()
    if args.json:
        print(json.dumps(data, indent=2)); return
    print("Python:", data["python"])
    print("Config:", data["config"])
    if data["jupyter"]:
        print("Jupyter:", data["jupyter"]["url"], "PID", data["jupyter"]["pid"])
    else: print("Jupyter: no linked running server")
    if data["viewer"]:
        print("Viewer:", "running" if data["viewer"].get("running") else "stopped",
              "127.0.0.1:" + str(data["viewer"]["port"]), "PID", data["viewer"]["pid"])
        print("Viewer URL:", data["viewer"]["public_url"])
    else: print("Viewer: not started")
    print("Active browser port:", data["active_browser_port"] or "not detected")
    print("Next-start settings:", json.dumps(data["configured"]))


def ports(arguments):
    if not arguments or arguments == ["--json"]:
        return status(arguments)
    parser = argparse.ArgumentParser(prog="fitsview ports")
    sub = parser.add_subparsers(dest="action", required=True)
    setter = sub.add_parser("set", help="Save ports for future starts; existing listeners and SSH tunnels stay active")
    for flag in ["jupyter", "browser", "viewer", "proxy", "ssh-port"]:
        setter.add_argument("--" + flag, type=port_value)
    setter.add_argument("--ssh-host")
    args = parser.parse_args(arguments)
    names = {"jupyter": "jupyter_port", "browser": "browser_port", "viewer": "viewer_port",
             "proxy": "proxy_port", "ssh_port": "ssh_port", "ssh_host": "ssh_host"}
    changes = {destination: getattr(args, source) for source, destination in names.items()
               if any(a == "--" + source.replace("_", "-") or a.startswith("--" + source.replace("_", "-") + "=") for a in arguments)}
    if not changes: parser.error("Specify at least one port or SSH host")
    settings = save_settings(changes)
    print("Saved:", config_path())
    print(json.dumps(settings, indent=2))
    print("Settings apply to future starts. Run fitsview tunnel for the client SSH command.")


def tunnel(arguments):
    parser = argparse.ArgumentParser(prog="fitsview tunnel", description="Print an SSH forwarding command to run on the computer with your browser.")
    parser.add_argument("--ssh-host")
    parser.add_argument("--ssh-port", type=port_value)
    parser.add_argument("--local-port", type=port_value)
    parser.add_argument("--remote-port", type=port_value)
    args = parser.parse_args(arguments)
    data = snapshot()
    settings = data["configured"]
    host = args.ssh_host or settings["ssh_host"]
    if not host: parser.error("Specify --ssh-host user@server, or save it with fitsview ports set")
    from .settings import validate
    validate({"ssh_host": host})
    remote = args.remote_port or (data["jupyter"] or {}).get("port") or settings["jupyter_port"]
    local = args.local_port or settings["browser_port"] or data["active_browser_port"] or remote
    ssh_port = args.ssh_port or settings["ssh_port"]
    if remote is None: parser.error("No running Jupyter; specify --remote-port")
    command = ["ssh", "-N", "-o", "ExitOnForwardFailure=yes", "-o", "ServerAliveInterval=30",
               "-L", f"127.0.0.1:{local}:127.0.0.1:{remote}", "-p", str(ssh_port), host]
    print(shlex.join(command))
    print(f"# Run on the browser computer; then open http://127.0.0.1:{local}/")
    print("# This prints a command; it does not change an existing SSH tunnel.")


def _free_port():
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        return probe.getsockname()[1]


def _login_url(server, browser_port):
    address = urlsplit(server["url"])
    netloc = "127.0.0.1:" + str(browser_port or server["port"])
    path = server.get("base_url", "/").rstrip("/") + "/lab"
    return urlunsplit((address.scheme, netloc, path, urlencode({"token": server.get("token", "")}), ""))


def jupyter_command(arguments):
    parser = argparse.ArgumentParser(prog="fitsview jupyter")
    sub = parser.add_subparsers(dest="action", required=True)
    starter = sub.add_parser("start", help="Start or reuse Jupyter in this Python environment")
    starter.add_argument("--port", type=port_value)
    starter.add_argument("--browser-port", type=port_value)
    starter.add_argument("--root", type=Path)
    sub.add_parser("url", help="Print the linked server login URL (contains its token)")
    args = parser.parse_args(arguments)
    settings = load_settings()
    server = jupyter_info()
    if args.action == "url":
        if not server: parser.error("No linked running Jupyter")
        print(_login_url(server, server.get("fitsview_browser_port") or snapshot()["active_browser_port"])); return
    explicit_port = any(a == "--port" or a.startswith("--port=") for a in arguments)
    explicit_browser = any(a == "--browser-port" or a.startswith("--browser-port=") for a in arguments)
    desired = args.port if explicit_port else settings["jupyter_port"]
    browser = args.browser_port if explicit_browser else settings["browser_port"]
    root = args.root.expanduser().resolve() if args.root else (Path(server["root_dir"]) if server else Path.cwd())
    if not root.is_dir(): parser.error("Root directory does not exist")
    if server and (desired is None or server["port"] == desired) and Path(server["root_dir"]).resolve() == root:
        print("Reusing Jupyter PID", server["pid"])
        print(_login_url(server, browser)); return
    port = desired or _free_port()
    with socket.socket() as probe:
        try: probe.bind(("127.0.0.1", port))
        except OSError: parser.error(f"Port {port} is busy; choose another port or --port auto")
    browser = browser or port
    environment = os.environ.copy()
    environment["FITSVIEW_BROWSER_ORIGIN"] = f"http://127.0.0.1:{browser}"
    environment["FITSVIEW_ROOT"] = str(root)
    executable = Path(sys.executable).parent / "jupyter-lab"
    if not executable.is_file(): parser.error("JupyterLab is not installed in this Python environment")
    directory = runtime_dir()
    logs = directory / "logs"
    logs.mkdir(exist_ok=True)
    from jupyter_core.paths import jupyter_runtime_dir
    command = [str(executable), "--no-browser", "--ip=127.0.0.1", f"--port={port}",
               "--ServerApp.port_retries=0", "--ServerApp.root_dir=" + str(root)]
    if os.geteuid() == 0: command.append("--allow-root")
    with (logs / "jupyter.log").open("ab") as log:
        process = subprocess.Popen(command, env=environment, cwd=root, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    info = Path(jupyter_runtime_dir()) / f"jpserver-{process.pid}.json"
    for _ in range(120):
        if info.is_file():
            server = json.loads(info.read_text())
            (directory / "jupyter.json").write_text(json.dumps({"pid": process.pid, "runtime_dir": str(info.parent), "browser_port": browser}))
            print("Started Jupyter PID", process.pid, "port", port)
            print(_login_url(server, browser))
            print("Use fitsview tunnel to generate an SSH command for your browser computer.")
            return
        if process.poll() is not None: raise RuntimeError("Jupyter failed to start; see " + str(logs / "jupyter.log"))
        time.sleep(0.25)
    process.terminate()
    raise TimeoutError("Jupyter startup timed out; see " + str(logs / "jupyter.log"))
