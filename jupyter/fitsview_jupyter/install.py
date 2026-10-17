"""Install this complete distribution into a selected Python environment.

把当前完整安装包复制到指定环境，无需目标环境安装 Rust 或 Node.js。
"""
import argparse
import base64
import csv
import hashlib
import importlib.metadata
import io
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import zipfile


def _target_python(args):
    if args.python:
        value = os.path.expanduser(args.python)
        path = Path(shutil.which(value) or value).absolute()
    else:
        prefix = args.prefix
        if args.conda_env:
            conda = args.conda_exe or os.environ.get("CONDA_EXE") or shutil.which("conda")
            if not conda: raise ValueError("Conda not found; use --conda-exe or --prefix")
            if args.conda_env == "base":
                prefix = subprocess.check_output([conda, "info", "--base"], text=True).strip()
            else:
                envs = json.loads(subprocess.check_output([conda, "env", "list", "--json"], text=True))["envs"]
                matches = [p for p in envs if Path(p).name == args.conda_env]
                if len(matches) != 1: raise ValueError("Conda name is missing or ambiguous; use --prefix with its full path")
                prefix = matches[0]
        directory = Path(prefix).expanduser().absolute()
        path = directory / "bin/python"
        if not path.is_file(): path = directory / "bin/python3"
    # Do not resolve this symlink: venv Python needs its original executable path.
    # 不解析 python 的软链接，否则可能误用基础环境。
    if not path.is_file(): raise FileNotFoundError("Target Python not found: " + str(path))
    return path


def _interactive_target(args, parser):
    if any([args.python, args.prefix, args.conda_env]): return
    if not sys.stdin.isatty(): parser.error("Select --python, --prefix or --conda-env; interactive selection needs a terminal")
    print("Installation target:\n  1. Current Python environment\n  2. A Python executable\n  3. A Conda environment")
    choice = input("Choose [1/2/3] (default 1): ").strip() or "1"
    if choice == "1": args.python = sys.executable
    elif choice == "2": args.python = input("Python executable path: ").strip()
    elif choice == "3": args.conda_env = input("Conda environment name (or base): ").strip()
    else: parser.error("Choose 1, 2 or 3")
    if not any([args.python, args.conda_env]): parser.error("Installation target cannot be empty")


def _choice(prompt, default):
    value = input(prompt + (" [Y/n]: " if default else " [y/N]: ")).strip().lower()
    if value in {"y", "yes"}: return "yes"
    if value in {"n", "no"}: return "no"
    if not value: return "yes" if default else "no"
    raise ValueError("Answer yes or no")


def _target_environment():
    # Inspect the selected environment, not this launcher's development overrides.
    # 检查目标环境，避免继承启动器的开发路径和端口配置。
    environment = {k: v for k, v in os.environ.items()
                   if not k.startswith("FITSVIEW_") and k not in {"PYTHONPATH", "PYTHONHOME"}}
    source_config = str(Path(__file__).resolve().parents[2] / "jupyter/jupyter-config")
    paths = environment.get("JUPYTER_CONFIG_PATH", "").split(os.pathsep)
    paths = [p for p in paths if p and str(Path(p).resolve()) != source_config]
    if paths: environment["JUPYTER_CONFIG_PATH"] = os.pathsep.join(paths)
    else: environment.pop("JUPYTER_CONFIG_PATH", None)
    return environment


def _inspect(python):
    code = """import importlib.metadata as m,json,sys
def version(name):
    try:return m.version(name)
    except m.PackageNotFoundError:return None
print(json.dumps({'python':sys.executable,'prefix':sys.prefix,'python_version':list(sys.version_info[:3]),'jupyterlab':version('jupyterlab'),'fitsview':version('fitsview-jupyter')}))
"""
    return json.loads(subprocess.check_output([str(python), "-c", code], text=True,
                                             env=_target_environment()))


def installation_details():
    """Read target ports and effective extension configuration without starting Jupyter.

    只读检查目标环境的端口及扩展配置，不启动或重启 Jupyter。
    """
    from jupyter_core.paths import jupyter_config_path, jupyter_path
    from jupyter_server.extension.config import ExtensionConfigManager
    from jupyter_server.serverapp import ServerApp
    from jupyterlab.commands import get_app_dir, get_page_config
    from .management import snapshot

    extensions = ExtensionConfigManager(read_config_path=jupyter_config_path()).get_jpserver_extensions()
    app = ServerApp()
    app.load_config_file(suppress_errors=False)
    extensions.update(app.jpserver_extensions)
    page = get_page_config(jupyter_path("labextensions"), str(Path(get_app_dir()) / "settings"))
    reasons = []
    for name in ["fitsview_jupyter", "jupyter_server_proxy"]:
        if not extensions.get(name): reasons.append(name + " server extension is not enabled")
    if not any(item["name"] == "fitsview-jupyter" for item in page.get("federated_extensions", [])):
        reasons.append("FITSView JupyterLab extension is missing")
    for pattern in page.get("disabledExtensions", []):
        try: disabled = any(re.search(pattern, name) for name in ["fitsview-jupyter", "fitsview-jupyter:viewer"])
        except re.error: disabled = pattern in {"fitsview-jupyter", "fitsview-jupyter:viewer"}
        if disabled:
            reasons.append("FITSView JupyterLab extension is disabled")
            break
    from .desktop import details
    return {"state": snapshot(), "binding": {"enabled": not reasons, "reasons": reasons}, "desktop": details()}


def _print_success(installed, details):
    state = details["state"]
    settings = state["configured"]
    port = settings["jupyter_port"] or "auto"
    browser = settings["browser_port"] or port
    print("\nFITSView installation successful (version " + installed["fitsview"] + ").")
    print("Installation environment:", installed["prefix"])
    print("Python executable:", installed["python"])
    print("JupyterLab port:", port, "(configured)")
    print("Browser access port:", browser, "(configured" + ("; same as JupyterLab)" if settings["browser_port"] is None else ")"))
    print("Standalone viewer port:", settings["viewer_port"] or "auto", "(configured)")
    print("JupyterLab viewer port:", settings["proxy_port"] or "auto", "(assigned when the viewer starts)")
    print("Running JupyterLab port:", state["jupyter"]["port"] if state["jupyter"] else "none detected in this environment")
    binding = details["binding"]
    print("JupyterLab binding:", "Yes (extensions enabled in configuration)" if binding["enabled"] else "No (" + "; ".join(binding["reasons"]) + ")")
    print("Default FITS viewer in JupyterLab:", "Yes" if settings.get("default_viewer", True) else "No (Open With remains available)")
    desktop = details.get("desktop", {})
    print("Linux desktop registration:", "Yes" if desktop.get("registered") else "No")
    print("Linux desktop default FITS viewer:", "Yes" if desktop.get("defaults", {}).get("image/fits") == "fitsview.desktop" else "No")
    print("Bind or re-enable JupyterLab with:")
    python = [installed["python"], "-m", "jupyter"]
    for name in ["fitsview_jupyter", "jupyter_server_proxy"]:
        print("  " + shlex.join([*python, "server", "extension", "enable", "--py", name, "--sys-prefix"]))
    print("  " + shlex.join([*python, "labextension", "enable", "fitsview-jupyter", "fitsview-jupyter:viewer", "--level=sys_prefix"]))
    print("Restart JupyterLab from this environment and refresh your browser to apply the binding.")
    print("Inspect ports:", shlex.join([str(Path(installed["prefix"]) / "bin/fitsview"), "ports"]))


def _supported(info):
    value = info["jupyterlab"]
    if not value: return False
    numbers = value.split(".")
    return numbers[0] == "4" and int(numbers[1]) >= 4


def _wheel(directory, explicit):
    if explicit:
        path = Path(explicit).expanduser().absolute()
        if not path.is_file() or path.suffix != ".whl": raise ValueError("--wheel must point to a local wheel")
        return path
    source = Path(__file__).resolve().parents[2]
    if (source / "Cargo.toml").is_file():
        candidates = list((source / "dist").glob("fitsview_jupyter-*.whl"))
        if candidates: return max(candidates, key=lambda p: p.stat().st_mtime)
        raise FileNotFoundError("Build a wheel first, or specify --wheel")
    distribution = importlib.metadata.distribution("fitsview-jupyter")
    package = Path(__file__).resolve().parent
    if not (package / "_vendor/bin/fitsview").is_file(): raise ValueError("This installation does not contain the complete Rust runtime")
    wheel_info = distribution.read_text("WHEEL")
    tags = [line[5:] for line in wheel_info.splitlines() if line.startswith("Tag: ")]
    if not tags or "linux" not in tags[0]: raise ValueError("Cannot reconstruct a Linux wheel from this installation")
    version = distribution.version
    name = "fitsview_jupyter-" + version
    path = directory / (name + "-" + tags[0] + ".whl")
    records = []
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        def add(destination, data, mode=0o644):
            item = zipfile.ZipInfo(destination)
            item.compress_type = zipfile.ZIP_DEFLATED
            item.external_attr = (0o100000 | mode) << 16
            archive.writestr(item, data)
            digest = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
            records.append([destination, "sha256=" + digest, str(len(data))])
        for file in sorted(package.rglob("*")):
            if file.is_file() and not file.is_symlink() and "__pycache__" not in file.parts:
                add("fitsview_jupyter/" + file.relative_to(package).as_posix(), file.read_bytes(), file.stat().st_mode & 0o777)
        for file in sorted((package / "labextension").rglob("*")):
            if file.is_file():
                add(name + ".data/data/share/jupyter/labextensions/fitsview-jupyter/" + file.relative_to(package / "labextension").as_posix(), file.read_bytes())
        config = {"ServerApp": {"jpserver_extensions": {"fitsview_jupyter": True}}}
        add(name + ".data/data/etc/jupyter/jupyter_server_config.d/fitsview-jupyter.json", json.dumps(config).encode())
        for file in ["METADATA", "WHEEL", "entry_points.txt"]:
            text = distribution.read_text(file)
            if text is not None: add(name + ".dist-info/" + file, text.encode())
        for file in distribution.files or []:
            if ".dist-info/licenses/" in file.as_posix():
                suffix = file.as_posix().split(".dist-info/", 1)[1]
                add(name + ".dist-info/" + suffix, Path(distribution.locate_file(file)).read_bytes())
        record = name + ".dist-info/RECORD"
        output = io.StringIO(newline="")
        csv.writer(output, lineterminator="\n").writerows(records + [[record, "", ""]])
        archive.writestr(record, output.getvalue())
    return path


def install(arguments):
    parser = argparse.ArgumentParser(prog="fitsview install", description="Install FITSView into a selected JupyterLab Python environment")
    target = parser.add_mutually_exclusive_group()
    target.add_argument("--python", help="Target Python executable; venv symlinks are preserved")
    target.add_argument("--prefix", help="Conda/venv environment directory")
    target.add_argument("--conda-env", help="Existing Conda environment name, including base")
    parser.add_argument("--conda-exe", help="Path to conda when it is not on PATH")
    parser.add_argument("--wheel", help="Optional local wheel; otherwise use this installed distribution")
    parser.add_argument("--check", action="store_true", help="Inspect target only; do not install")
    parser.add_argument("--with-jupyter", action="store_true", help="Allow pip to install/upgrade JupyterLab 4.4+")
    parser.add_argument("--no-deps", action="store_true", help="Do not install dependencies (advanced)")
    parser.add_argument("--default-viewer", choices=["yes", "no"], help="Default FITS viewer in JupyterLab; preserve existing setting in noninteractive mode if omitted")
    parser.add_argument("--desktop-default", choices=["yes", "no"], help="Register native desktop as default, or restore previous desktop defaults")
    args = parser.parse_args(arguments)
    _interactive_target(args, parser)
    python = _target_python(args)
    info = _inspect(python)
    if info["python_version"] < [3, 10, 0]: raise ValueError("Python 3.10+ is required")
    if args.check:
        info["supported_jupyterlab"] = _supported(info)
        print(json.dumps(info, indent=2))
        return 0 if info["supported_jupyterlab"] else 1
    if not _supported(info) and not args.with_jupyter:
        raise ValueError("Target needs JupyterLab 4.4+ (<5). Use --with-jupyter to install or upgrade it.")
    if sys.stdin.isatty():
        if args.default_viewer is None: args.default_viewer = _choice("Make FITSView the default FITS viewer in JupyterLab?", True)
        if args.desktop_default is None: args.desktop_default = _choice("Make FITSView the default FITS viewer on this Linux desktop?", False)
    with tempfile.TemporaryDirectory(prefix="fitsview-install-") as temporary:
        wheel = _wheel(Path(temporary), args.wheel)
        command = [str(python), "-m", "pip", "install", "--upgrade"]
        if args.no_deps: command.append("--no-deps")
        subprocess.run([*command, str(wheel)], check=True, env=_target_environment())
    installed = _inspect(python)
    if args.default_viewer is not None:
        code = "from fitsview_jupyter.settings import save_settings; save_settings({'default_viewer': " + str(args.default_viewer == "yes") + "})"
        subprocess.run([str(python), "-c", code], env=_target_environment(), check=True)
    if args.desktop_default is not None:
        action = "install()" if args.desktop_default == "yes" else "disable_default()"
        subprocess.run([str(python), "-c", "from fitsview_jupyter import desktop; desktop." + action], env=_target_environment(), check=True)
    code = "import json; from fitsview_jupyter.install import installation_details; print(json.dumps(installation_details()))"
    details = json.loads(subprocess.check_output([str(python), "-c", code], text=True,
                                                env=_target_environment()))
    _print_success(installed, details)
    return 0
