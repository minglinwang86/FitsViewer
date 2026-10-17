"""Bind the viewer to its Jupyter server. / 将查看器绑定到实际的 Jupyter 服务。"""
import json
import os
from pathlib import Path
from urllib.parse import urlsplit, urlunsplit
from .runtime import process_environment, resources, runtime_dir
from .settings import load_settings
from jupyter_server.base.handlers import APIHandler
from jupyter_server.utils import url_path_join
from tornado.web import authenticated

_serverapp = None


class ViewerSettingsHandler(APIHandler):
    @authenticated
    def get(self):
        self.finish({"default_viewer": load_settings()["default_viewer"]})


def _jupyter_server_extension_points():
    return [{"module": "fitsview_jupyter"}]


def _load_jupyter_server_extension(serverapp):
    global _serverapp
    _serverapp = serverapp
    serverapp.web_app.add_handlers(".*$", [(url_path_join(serverapp.base_url, "fitsview-settings"), ViewerSettingsHandler)])
    # Keep only a pointer to Jupyter's own private runtime record, never copy its token.
    # 仅记录 Jupyter 运行信息的位置，不复制认证 token。
    origin = os.environ.get("FITSVIEW_BROWSER_ORIGIN")
    browser_port = urlsplit(origin).port if origin else load_settings()["browser_port"]
    state = {"pid": os.getpid(), "runtime_dir": str(serverapp.runtime_dir), "browser_port": browser_port}
    (runtime_dir() / "jupyter.json").write_text(json.dumps(state))
    os.environ["FITSVIEW_JUPYTER_BASE_URL"] = serverapp.base_url
    serverapp.log.info("FITSView: default FITS viewer available in JupyterLab")


def _command(port, base_url):
    _, binary, web = resources()
    root = os.environ.get("FITSVIEW_ROOT") or (_serverapp.root_dir if _serverapp else str(Path.cwd()))
    origin = os.environ.get("FITSVIEW_BROWSER_ORIGIN")
    if not origin and load_settings()["browser_port"]:
        origin = "http://127.0.0.1:" + str(load_settings()["browser_port"])
    if origin:
        public_url = origin.rstrip("/") + base_url.rstrip("/") + "/fitsview/"
    elif _serverapp:
        address = urlsplit(_serverapp.connection_url)
        public_url = urlunsplit((address.scheme, address.netloc, base_url.rstrip("/") + "/fitsview/", "", ""))
    else:
        public_url = base_url.rstrip("/") + "/fitsview/"
    return [str(binary), "--serve", "--port", str(port), "--root", str(root),
            "--web-dir", str(web), "--jupyter", "--public-url", public_url]


def setup_server():
    """Jupyter Server Proxy starts the packaged viewer on demand with matching roots."""
    config = {"command": _command, "environment": process_environment, "timeout": 30,
              "launcher_entry": {"title": "FITSView", "category": "Other"}}
    if load_settings()["proxy_port"]:
        config["port"] = load_settings()["proxy_port"]
    return config
