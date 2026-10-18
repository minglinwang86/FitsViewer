"""Installed module metadata; does not import Jupyter or scientific packages."""
import json
from pathlib import Path


def build_info():
    path = Path(__file__).with_name("build-profile.json")
    if not path.exists():
        return {"schema": 1, "profile": "development", "modules": ["base", "pro", "desktop", "jupyter"], "pro": True, "desktop": True, "jupyter": True}
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("schema") != 1 or not isinstance(value.get("modules"), list) or "base" not in value["modules"]:
        raise RuntimeError("Invalid installed FitsViewer build profile")
    if any(not isinstance(name, str) for name in value["modules"]) or set(value["modules"]) - {"base", "pro", "desktop", "jupyter"}:
        raise RuntimeError("Unknown installed FitsViewer build module")
    for name in ["pro", "desktop", "jupyter"]:
        if not isinstance(value.get(name), bool) or value[name] != (name in value["modules"]):
            raise RuntimeError("Inconsistent installed FitsViewer module: " + name)
    return value


def require_module(name):
    if name not in build_info()["modules"]:
        raise RuntimeError("This FitsViewer build does not include the " + name + " module")
