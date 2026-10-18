"""Change Jupyter and desktop defaults. / 设置 Jupyter 与桌面的默认查看器。"""
import argparse
import json
from . import desktop
from .settings import load_settings, save_settings


def status():
    return {"jupyter_default": load_settings()["default_viewer"], "desktop": desktop.details()}


def command(arguments):
    parser = argparse.ArgumentParser(prog="fitsviewer defaults")
    sub = parser.add_subparsers(dest="action", required=True)
    sub.add_parser("status")
    setter = sub.add_parser("set")
    setter.add_argument("--jupyter", choices=["yes", "no"])
    setter.add_argument("--desktop", choices=["yes", "no"])
    args = parser.parse_args(arguments)
    if args.action == "set":
        from .build_info import require_module
        if args.jupyter == "yes": require_module("jupyter")
        if args.desktop == "yes": require_module("desktop")
        if args.jupyter is None and args.desktop is None: parser.error("Choose --jupyter and/or --desktop yes|no")
        if args.jupyter is not None: save_settings({"default_viewer": args.jupyter == "yes"})
        if args.desktop == "yes": desktop.install()
        elif args.desktop == "no": desktop.disable_default()
    print(json.dumps(status(), indent=2))
    if args.action == "set": print("Refresh JupyterLab to apply its default viewer setting. Open With remains available.")
