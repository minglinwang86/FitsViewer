"""Locate installed resources and writable state. / 定位安装资源与用户运行目录。"""
import hashlib
import os
from pathlib import Path
import sys


def resources():
    """Return (resource root, executable, web directory), allowing source development."""
    override = os.environ.get("FITSVIEW_HOME")
    if override:
        root = Path(override).expanduser().resolve()
        binary = root / ("target/release/fitsview" if (root / "Cargo.toml").exists() else "bin/fitsview")
    else:
        root = Path(__file__).resolve().parent / "_vendor"
        binary = root / "bin/fitsview"
    web = root / "web"
    if not binary.is_file() or not (web / "pkg/fitsview_web_bg.wasm").is_file():
        raise FileNotFoundError("FITSView runtime is missing. Install the complete Linux wheel, or build the source project.")
    return root, binary, web


def runtime_dir():
    """Use writable user state, scoped to this Python environment."""
    override = os.environ.get("FITSVIEW_RUNTIME_DIR")
    if override:
        directory = Path(override).expanduser()
    elif os.environ.get("FITSVIEW_HOME") and (Path(os.environ["FITSVIEW_HOME"]) / "Cargo.toml").exists():
        directory = Path(os.environ["FITSVIEW_HOME"]) / ".runtime"
    else:
        cache = Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache")))
        environment = hashlib.sha256(str(Path(sys.prefix).resolve()).encode()).hexdigest()[:16]
        directory = cache / "fitsview" / environment
    directory.mkdir(parents=True, exist_ok=True, mode=0o700)
    return directory.resolve()


def process_environment():
    """Environment shared by the CLI and the Jupyter-supervised Rust process."""
    root, _, _ = resources()
    return {"FITSVIEW_HOME": str(root), "FITSVIEW_RUNTIME_DIR": str(runtime_dir())}
