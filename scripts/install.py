"""Install a wheel or build this Git checkout into a selected environment.

纯命令行安装：支持 wheel、Git 源码、Python/Conda 和默认查看器选项。
"""
from pathlib import Path
import argparse
import importlib.util
import os
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(add_help=False)
parser.add_argument("--from-source", action="store_true")
parser.add_argument("--build-python", default=sys.executable)
args, installation = parser.parse_known_args()
if args.from_source and not any(a in {"--check", "--help", "-h"} for a in installation):
    if "--wheel" in installation: raise SystemExit("Choose --from-source or --wheel")
    subprocess.run(["bash", str(root / "scripts/bootstrap.sh"), args.build_python, "--no-qa"], cwd=root, check=True)
    subprocess.run(["bash", str(root / "scripts/build.sh"), "--portable", "--repair"], cwd=root, check=True)
spec = importlib.util.spec_from_file_location("fitsview_installer", root / "jupyter/fitsview_jupyter/install.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
raise SystemExit(module.install(installation))
