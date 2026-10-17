"""Jupyter integration; numeric FITS processing lives in Rust. / 数值处理由 Rust 完成。"""
import os
from pathlib import Path
from urllib.parse import urlencode
from .server import setup_server, _jupyter_server_extension_points, _load_jupyter_server_extension


def show(path, hdu=None, height=650, base_url=None):
    """Return an interactive notebook IFrame backed by the same viewer.

    返回交互式 Notebook 内嵌视图。path 为 Jupyter 根目录相对路径或授权根内的绝对路径。
    Set base_url for JupyterHub or a server hosted under a URL prefix.

    Args:
        path (str | Path): FITS path relative to the Jupyter root, or an allowed absolute path.
        hdu (int | None): Zero-based HDU index; None selects the first ordinary 2D image.
        height (int): IFrame height in pixels, at least 200.
        base_url (str | None): Jupyter URL prefix; defaults to the environment or '/'.

    Returns:
        IPython.display.IFrame: A live viewer served through authenticated Jupyter access.

    Raises:
        ValueError: The height or HDU index is invalid.
    """
    from IPython.display import IFrame

    if height < 200:
        raise ValueError("height must be at least 200 pixels")
    query = {"path": str(path)}
    if hdu is not None:
        if not isinstance(hdu, int) or hdu < 0:
            raise ValueError("hdu must be a non-negative integer")
        query["hdu"] = hdu
    prefix = base_url if base_url is not None else os.environ.get("FITSVIEWER_JUPYTER_BASE_URL", "/")
    url = prefix.rstrip("/") + "/fitsviewer/?" + urlencode(query)
    return IFrame(src=url, width="100%", height=height)


__all__ = ["show", "setup_server"]
