"""The pip-installed fitsview command. / pip 安装后的 fitsview 命令入口。"""
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.error
import urllib.request
from .runtime import process_environment, resources, runtime_dir
from .settings import load_settings


def _warm_jupyter():
    try:
        from .management import jupyter_info
        server = jupyter_info()
        if not server: return
        url = server["url"].rstrip("/") + "/fitsview/api/health"
        request = urllib.request.Request(url, headers={"Authorization": "token " + server.get("token", "")})
        with urllib.request.urlopen(request, timeout=30) as response:
            if json.load(response).get("viewer") != "fitsview":
                raise ValueError("Unexpected viewer response")
    except (OSError, ValueError, KeyError, urllib.error.URLError):
        pass  # No active authenticated Jupyter: let the Rust CLI start a standalone viewer.


def main():
    """Run the bundled Linux program; warm an existing Jupyter viewer when available."""
    arguments = sys.argv[1:]
    if arguments and arguments[0] in {"status", "ports", "tunnel", "jupyter", "install", "desktop", "defaults"}:
        try:
            if arguments[0] == "install":
                from .install import install
                raise SystemExit(install(arguments[1:]))
            if arguments[0] in {"desktop", "defaults"}:
                from importlib import import_module
                import_module("fitsview_jupyter." + arguments[0]).command(arguments[1:])
                return
            from . import management
            handler = {"status": management.status, "ports": management.ports,
                       "tunnel": management.tunnel, "jupyter": management.jupyter_command}[arguments[0]]
            handler(arguments[1:])
            return
        except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
            print("FITSView:", str(error), file=sys.stderr)
            raise SystemExit(1) from error
    _, binary, web = resources()
    if arguments in (["--help"], ["-h"]):
        print("Management commands:\n  status / ports [--json]\n  ports set --browser PORT --jupyter PORT\n  tunnel --ssh-host USER@HOST\n  jupyter start [--port PORT] [--browser-port PORT] [--root PATH]\n  jupyter url\n  install [--python PATH | --prefix PATH | --conda-env NAME]\n  desktop install [--no-default] / remove / status\n  defaults status / set --jupyter yes|no --desktop yes|no\n")
    if not any(a == "--port" or a.startswith("--port=") for a in arguments):
        arguments += ["--port", str(load_settings()["viewer_port"] or 0)]
    if not any(a in {"--native", "--serve", "--help", "-h", "--version", "-V"} for a in arguments):
        _warm_jupyter()
    environment = os.environ.copy()
    environment.update(process_environment())
    raise SystemExit(subprocess.call([str(binary), *arguments, "--web-dir", str(web)], env=environment))


if __name__ == "__main__":
    main()
