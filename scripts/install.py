"""Install a wheel or build this Git checkout into a selected environment.

纯命令行安装：支持 wheel、Git 源码、Python/Conda 和默认查看器选项。
"""
from pathlib import Path
import argparse
import importlib.util
import os
import subprocess
import sys
import types

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(add_help=False)
parser.add_argument("--from-source", action="store_true")
parser.add_argument("--build-python", default=sys.executable)
args, installation = parser.parse_known_args()
if args.from_source and not any(a in {"--check", "--help", "-h"} for a in installation):
    if "--wheel" in installation: raise SystemExit("Choose --from-source or --wheel")
    subprocess.run(["bash", str(root / "scripts/bootstrap.sh"), args.build_python, "--no-qa"], cwd=root, check=True)
    subprocess.run(["bash", str(root / "scripts/build.sh"), "--portable", "--repair"], cwd=root, check=True)
# Supply a package context for the installer's relative standard-library helpers.
# 提供包上下文以加载相对辅助模块，无需在启动安装器的环境预装 Jupyter。
package = types.ModuleType('fitsviewer')
package.__path__ = [str(root / 'jupyter/fitsviewer')]
sys.modules['fitsviewer'] = package
spec = importlib.util.spec_from_file_location("fitsviewer.install", root / "jupyter/fitsviewer/install.py")
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
raise SystemExit(module.install(installation))
