"""Persistent per-environment CLI settings. / 每个 Python 环境独立保存的命令配置。"""
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile

DEFAULTS = {"jupyter_port": 8082, "browser_port": None, "viewer_port": 8765,
            "proxy_port": None, "ssh_port": 22, "ssh_host": None, "default_viewer": True}


def config_path():
    override = os.environ.get("FITSVIEWER_CONFIG")
    if override:
        return Path(override).expanduser().resolve()
    home = os.environ.get("FITSVIEWER_HOME")
    if home and (Path(home) / "Cargo.toml").is_file():
        return Path(home).resolve() / ".runtime/config.json"
    fallback = Path(os.environ.get("LOCALAPPDATA", str(Path.home()))) / "FitsViewer/config" if os.name=='nt' else Path.home()/'.config'
    base = Path(os.environ.get("XDG_CONFIG_HOME", str(fallback)))
    environment = hashlib.sha256(str(Path(sys.prefix).resolve()).encode()).hexdigest()[:16]
    return base / "fitsviewer" / (environment + ".json")


def validate(settings):
    unknown = set(settings) - set(DEFAULTS)
    if unknown:
        raise ValueError("Unknown settings: " + ", ".join(sorted(unknown)))
    for key, value in settings.items():
        if key == "default_viewer":
            if type(value) is not bool: raise ValueError("default_viewer must be true or false")
        elif key == "ssh_host":
            if value is not None and (not isinstance(value, str) or not value or value.startswith("-") or any(c.isspace() for c in value)):
                raise ValueError("ssh_host must be a hostname or user@hostname")
        elif value is None:
            if key == "ssh_port": raise ValueError("ssh_port cannot be automatic")
        elif type(value) is not int or not 1 <= value <= 65535:
            raise ValueError(key + " must be an integer from 1 to 65535")


def load_settings():
    path = config_path()
    settings = dict(DEFAULTS)
    if path.exists():
        supplied = json.loads(path.read_text())
        if not isinstance(supplied, dict): raise ValueError("Configuration must be a JSON object")
        validate(supplied)
        settings.update(supplied)
    return settings


def save_settings(changes):
    validate(changes)
    settings = load_settings()
    settings.update(changes)
    path = config_path()
    path.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.NamedTemporaryFile("w", dir=path.parent, delete=False) as output:
        temporary = Path(output.name)
        json.dump(settings, output, indent=2)
        output.write("\n")
    os.replace(temporary, path)
    return settings


def port_value(value):
    if value.lower() in {"auto", "same"}: return None
    try: port = int(value)
    except ValueError as error: raise ValueError("Port must be 1..65535, auto, or same") from error
    if not 1 <= port <= 65535: raise ValueError("Port must be 1..65535")
    return port
