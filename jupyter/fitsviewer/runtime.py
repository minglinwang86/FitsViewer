"""Locate installed resources and writable state. / 定位安装资源与用户运行目录。"""
import hashlib
import os
from pathlib import Path
import sys


def resources():
    """Return (resource root, executable, web directory), allowing source development."""
    suffix = ".exe" if os.name == "nt" else ""
    override = os.environ.get("FITSVIEWER_HOME")
    if override:
        root = Path(override).expanduser().resolve()
        binary = root / (("target/release/fitsviewer" if (root / "Cargo.toml").exists() else "bin/fitsviewer") + suffix)
    else:
        root = Path(__file__).resolve().parent / "_vendor"
        binary = root / ("bin/fitsviewer" + suffix)
    web = root / "web"
    if not binary.is_file() or not (web / "pkg/fitsviewer_web_bg.wasm").is_file():
        raise FileNotFoundError("FitsViewer runtime is missing. Install the complete wheel for this platform, or build the source project.")
    return root, binary, web


def runtime_dir():
    """Use writable user state, scoped to this Python environment."""
    override = os.environ.get("FITSVIEWER_RUNTIME_DIR")
    if override:
        directory = Path(override).expanduser()
    elif os.environ.get("FITSVIEWER_HOME") and (Path(os.environ["FITSVIEWER_HOME"]) / "Cargo.toml").exists():
        directory = Path(os.environ["FITSVIEWER_HOME"]) / ".runtime"
    else:
        fallback = Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "FitsViewer/cache" if os.name == "nt" else Path.home() / ".cache"
        cache = Path(os.environ.get("XDG_CACHE_HOME", str(fallback)))
        environment = hashlib.sha256(str(Path(sys.prefix).resolve()).encode()).hexdigest()[:16]
        directory = cache / "fitsviewer" / environment
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    return directory.resolve()


def process_environment():
    """Environment shared by the CLI and the Jupyter-supervised Rust process."""
    root, _, _ = resources()
    return {"FITSVIEWER_HOME": str(root), "FITSVIEWER_RUNTIME_DIR": str(runtime_dir()),
            "FITSVIEWER_PYTHON": os.environ.get("FITSVIEWER_PYTHON") or sys.executable}
