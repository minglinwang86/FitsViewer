"""Jupyter integration; numeric FITS processing lives in Rust. / 数值处理由 Rust 完成。"""
import json,os
from pathlib import Path
from urllib.parse import urlencode
from .build_info import build_info, require_module


def setup_server():
    require_module("jupyter")
    from .server import setup_server as implementation
    return implementation()


def _jupyter_server_extension_points():
    if not build_info()["jupyter"]:
        return []
    from .server import _jupyter_server_extension_points as implementation
    return implementation()


def _load_jupyter_server_extension(serverapp):
    require_module("jupyter")
    from .server import _load_jupyter_server_extension as implementation
    return implementation(serverapp)


def viewer_url(path, hdu=None, base_url=None, mode=None, view=None):
    """Build a prefixed URL; mode is Minimal/PRO and view is a bounded JSON object.
    生成保留代理前缀的 URL，显示参数由 Rust 核心再次验证。
    """
    if not build_info()["pro"]:
        if mode=="PRO":raise ValueError("PRO is not included in this build")
        if mode is None:mode="Minimal"
    query={"path":str(path)}
    if hdu is not None:
        if not isinstance(hdu,int) or hdu<0:raise ValueError('hdu must be a non-negative integer')
        query['hdu']=hdu
    if mode is not None:
        if mode not in ['Minimal','PRO']:raise ValueError('mode must be Minimal or PRO')
        query['mode']=mode
    if view is not None:
        if not isinstance(view,dict):raise ValueError('view must be a JSON object')
        encoded=json.dumps(view,ensure_ascii=False,allow_nan=False,separators=(',',':'))
        if len(encoded.encode('utf-8'))>16384:raise ValueError('Embedded view parameters exceed 16 KiB')
        query['view']=encoded
    prefix=base_url if base_url is not None else os.environ.get('FITSVIEWER_JUPYTER_BASE_URL','/')
    return prefix.rstrip('/')+'/fitsviewer/?'+urlencode(query)


def show(path, hdu=None, height=650, base_url=None, mode=None, view=None):
    """Return an interactive notebook IFrame backed by the same viewer.

    返回交互式 Notebook 内嵌视图。path 为 Jupyter 根目录相对路径或授权根内的绝对路径。
    Set base_url for JupyterHub or a server hosted under a URL prefix.

    Args:
        path (str | Path): FITS path relative to the Jupyter root, or an allowed absolute path.
        hdu (int | None): Zero-based HDU index; None selects the first viewable image/spectrum.
        height (int): IFrame height in pixels, at least 200.
        base_url (str | None): Jupyter URL prefix; defaults to the environment or '/'.
        mode (str | None): Optional Minimal/PRO mode; None uses the saved preference.
        view (dict | None): Optional display configuration, limited to 16 KiB in the URL.

    Returns:
        IPython.display.IFrame: A live viewer served through authenticated Jupyter access.

    Raises:
        ValueError: The height, HDU, mode or serialized view is invalid.
    """
    require_module("jupyter")
    from IPython.display import IFrame

    if height < 200:
        raise ValueError("height must be at least 200 pixels")
    url = viewer_url(path,hdu=hdu,base_url=base_url,mode=mode,view=view)
    return IFrame(src=url, width="100%", height=height)


__all__ = ["show", "viewer_url", "setup_server", "build_info"]
