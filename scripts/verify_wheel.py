"""Install the wheel in a fresh venv, test a separate Jupyter, then uninstall it.

新建隔离环境测试 pip 安装、FITS 默认打开、命令行、内嵌和卸载，不重启已有 Jupyter。
"""
from pathlib import Path
import argparse
import importlib.metadata
import hashlib
import json
import os
import shutil
import shlex
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
import zipfile
from check_management import check_management, check_standalone
from check_desktop import check_desktop
from check_interactive_install import check_interactive_install

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("wheel", type=Path)
parser.add_argument("--jupyterlab", default=">=4.4,<5", help="JupyterLab constraint for this regression run")
args = parser.parse_args()
wheel = args.wheel.resolve()
report = {"wheel": wheel.name, "sha256": hashlib.sha256(wheel.read_bytes()).hexdigest()}
report.update(python=sys.version.split()[0], jupyterlab_constraint=args.jupyterlab)
with zipfile.ZipFile(wheel) as archive:
    for name in ["OFL.txt", "UFL.txt", "Hack-Regular.txt", "emoji-icon-font-mit-license.txt"]:
        assert any(n.endswith('/fonts/' + name) for n in archive.namelist()), 'Missing font notice: ' + name
    report['bundled_font_notices'] = True
for directory in ["logs", "reports/screenshots"]: (root / directory).mkdir(parents=True, exist_ok=True)

with tempfile.TemporaryDirectory(prefix="fitsview-pip-") as temporary:
    scratch = Path(temporary)
    venv = scratch / "venv"
    subprocess.run([sys.executable, "-m", "venv", str(venv)], check=True)
    python = str(venv / "bin/python")
    environment = {k: v for k, v in os.environ.items() if not k.startswith(("FITSVIEW_", "JUPYTER_", "PYTHON"))}
    environment.update(PATH=str(venv / "bin"), XDG_CACHE_HOME=str(scratch / "cache"), XDG_DATA_HOME=str(scratch / "desktop-data"),
                       XDG_CONFIG_HOME=str(scratch / "settings"),
                       JUPYTER_CONFIG_DIR=str(scratch / "config"), JUPYTER_DATA_DIR=str(scratch / "jupyter-data"),
                       JUPYTER_RUNTIME_DIR=str(scratch / "runtime"), JUPYTER_PREFER_ENV_PATH="1")
    downloaded = scratch / wheel.name
    shutil.copy2(wheel, downloaded)
    subprocess.run([python, "-m", "pip", "install", str(downloaded), "jupyterlab" + args.jupyterlab], env=environment, check=True)
    assert not any(shutil.which(tool, path=environment["PATH"]) for tool in ["cargo", "rustc", "node", "npm"])
    report["install_without_rust_node"] = True
    data = scratch / "independent data"
    data.mkdir()
    for name in ["science.fits", "second.fits"]:
        shutil.copy2(root / "examples/starfield.fits", data / name)
    report["management"] = check_management(venv, environment, data)
    target = scratch / "target env"
    subprocess.run([sys.executable, "-m", "venv", str(target)], check=True)
    target_environment = dict(environment, PATH=str(target / "bin"))
    subprocess.run([str(target / "bin/python"), "-m", "pip", "install", "jupyterlab" + args.jupyterlab], env=target_environment, check=True)
    downloaded.unlink()
    inspection = subprocess.check_output([str(venv / "bin/fitsview"), "install", "--python", str(target / "bin/python"), "--check"], env=environment, text=True)
    assert json.loads(inspection)["prefix"] == str(target)
    # Give the target different ports and the caller an explicit source config.
    # 目标端口和调用方不同，验证成功提示没有串用调用方的环境配置。
    target_config = Path(environment["XDG_CONFIG_HOME"]) / "fitsview" / (hashlib.sha256(str(target.resolve()).encode()).hexdigest()[:16] + ".json")
    target_config.parent.mkdir(parents=True, exist_ok=True)
    target_config.write_text(json.dumps({"jupyter_port": 8094, "browser_port": 18094, "viewer_port": 8766}))
    caller = json.loads(subprocess.check_output([str(venv / "bin/fitsview"), "ports", "--json"], env=environment, text=True))
    install_environment = dict(environment, FITSVIEW_CONFIG=caller["config"])
    command = [str(venv / "bin/fitsview"), "install", "--python", str(target / "bin/python")]
    result = subprocess.run(command, env=install_environment, check=True, capture_output=True, text=True, stdin=subprocess.DEVNULL)
    summary = result.stdout[result.stdout.index("FITSView installation successful"):]
    assert "Installation environment: " + str(target) in summary
    assert "Python executable: " + str(target / "bin/python") in summary
    assert "JupyterLab port: 8094 (configured)" in summary
    assert "Browser access port: 18094 (configured)" in summary
    assert "Standalone viewer port: 8766 (configured)" in summary
    assert "JupyterLab viewer port: auto" in summary
    assert "Running JupyterLab port: none detected in this environment" in summary
    assert "JupyterLab binding: Yes" in summary
    (root / "reports/install_success_example.txt").write_text(summary.replace(str(target), "/path/to/env"))
    report["installation_summary"] = {"target_environment": True, "target_ports": True,
                                      "caller_config_isolated": True, "binding_enabled": True}

    # Disable both integrations, reinstall, then execute the printed commands.
    # 关闭前后端后重新安装，并真实执行提示中的绑定命令。
    server_config = target / "etc/jupyter/jupyter_server_config.d/fitsview-jupyter.json"
    server_config.write_text(json.dumps({"ServerApp": {"jpserver_extensions": {"fitsview_jupyter": False}}}))
    page_config = target / "etc/jupyter/labconfig/page_config.json"
    page_config.parent.mkdir(parents=True, exist_ok=True)
    page_config.write_text(json.dumps({"disabledExtensions": {"fitsview-jupyter:viewer": True}}))
    result = subprocess.run([*command, "--no-deps"], env=install_environment, check=True, capture_output=True, text=True, stdin=subprocess.DEVNULL)
    disabled = result.stdout[result.stdout.index("FITSView installation successful"):]
    assert "JupyterLab binding: No" in disabled and "extension is disabled" in disabled
    assert "server extension is not enabled" in disabled
    binding_commands = [shlex.split(line.strip()) for line in disabled.splitlines() if line.startswith("  ")]
    assert len(binding_commands) == 3
    for binding_command in binding_commands:
        assert binding_command[0] == str(target / "bin/python")
        subprocess.run(binding_command, env=target_environment, check=True, capture_output=True, text=True)
    state = json.loads(subprocess.check_output([str(target / "bin/python"), "-c",
                        "import json; from fitsview_jupyter.install import installation_details; print(json.dumps(installation_details()))"],
                       env=target_environment, text=True))
    assert state["binding"]["enabled"], state["binding"]
    report["installation_summary"].update(binding_disabled_detected=True, printed_binding_commands_work=True,
                                           paths_with_spaces=True)
    inspection = subprocess.check_output([str(venv / "bin/fitsview"), "install", "--prefix", str(target), "--check"], env=environment, text=True)
    assert json.loads(inspection)["fitsview"] is not None
    report.update(install_into_target_python=True, install_without_original_wheel=True, target_venv_symlink_preserved=True)
    report["interactive_install"] = check_interactive_install(str(venv / "bin/fitsview"), target, install_environment)
    # The Git checkout installer also works without importing the app in its launcher.
    # Git 源码安装器共用环境和默认选项，并可直接安装预构建 wheel。
    source_command = [sys.executable, str(root / "scripts/install.py"), "--wheel", str(wheel),
                      "--python", str(target / "bin/python"), "--no-deps", "--default-viewer", "no", "--desktop-default", "no"]
    source_install = subprocess.run(source_command, env=target_environment, stdin=subprocess.DEVNULL,
                                    capture_output=True, text=True, check=True)
    assert "Default FITS viewer in JupyterLab: No" in source_install.stdout
    readonly = subprocess.check_output([sys.executable, str(root / "scripts/install.py"), "--from-source", "--prefix", str(target), "--check"],
                                      env=target_environment, text=True)
    assert json.loads(readonly)["prefix"] == str(target)
    subprocess.run([str(target / "bin/fitsview"), "defaults", "set", "--jupyter", "yes"], env=target_environment, check=True, capture_output=True)
    report["source_installer"] = {"prebuilt_wheel_mode": True, "explicit_default_options": True, "from_source_readonly_check": True}
    venv = target
    python = str(venv / "bin/python")
    environment = target_environment
    report["desktop"] = check_desktop(venv, environment, data / "science.fits")
    report["standalone_auto_port"] = check_standalone(venv, environment, data / "science.fits")
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0)); proxy_port = probe.getsockname()[1]
    subprocess.run([str(venv / "bin/fitsview"), "ports", "set", "--proxy", str(proxy_port)], env=environment, check=True, capture_output=True)
    subprocess.run([python, "-m", "ipykernel", "install", "--sys-prefix", "--name", "fitsview-wheel-test"], env=environment, check=True)
    notebook = {"nbformat": 4, "nbformat_minor": 5,
                "metadata": {"kernelspec": {"name": "fitsview-wheel-test", "display_name": "FITSView test", "language": "python"}},
                "cells": [{"cell_type": "code", "id": "viewer", "metadata": {}, "execution_count": None,
                           "outputs": [], "source": ["from fitsview_jupyter import show\n", "show('science.fits')"]}]}
    (data / "demo.ipynb").write_text(json.dumps(notebook))
    with socket.socket() as probe:
        probe.bind(("127.0.0.1", 0))
        port = probe.getsockname()[1]
    command = [str(venv / "bin/jupyter-lab"), "--no-browser", "--allow-root", "--ip=127.0.0.1",
               f"--port={port}", "--ServerApp.port_retries=0", "--ServerApp.base_url=/wheel-test/", "--LabApp.expose_app_in_browser=True", f"--ServerApp.root_dir={data}"]
    process = None
    runtime = None
    try:
        with (root / "logs/pip-jupyter.log").open("wb") as log:
            process = subprocess.Popen(command, env=environment, cwd=data, stdout=log, stderr=subprocess.STDOUT)
        info_file = Path(environment["JUPYTER_RUNTIME_DIR"]) / f"jpserver-{process.pid}.json"
        for _ in range(120):
            if info_file.exists(): break
            if process.poll() is not None: raise RuntimeError("Test Jupyter failed; inspect logs/pip-jupyter.log")
            time.sleep(0.5)
        assert info_file.exists(), "Test Jupyter did not become ready"
        # The pip command warms Jupyter before a FITS tab is opened.
        output = subprocess.check_output([str(venv / "bin/fitsview"), str(data / "science.fits"), "--no-browser"], env=environment, text=True)
        assert "/wheel-test/fitsview/?path=" in output
        runtime_path = Path(subprocess.check_output([python, "-c", "from fitsview_jupyter.runtime import runtime_dir; print(runtime_dir())"], env=environment, text=True).strip())
        runtime = json.loads((runtime_path / "server.json").read_text())
        assert runtime["port"] == proxy_port
        report["fixed_proxy_port"] = True
        assert Path(runtime["root"]) == data
        binary = Path(f"/proc/{runtime['pid']}/exe").resolve()
        assert venv in binary.parents and "_vendor/bin/fitsview" in str(binary)
        assert runtime_path.is_relative_to(scratch / "cache")
        report.update(packaged_binary=True, automatic_jupyter_root=True, writable_user_state=True, cli_warms_jupyter=True)
        config = {"server_info": str(info_file), "venv": str(venv), "data_root": str(data), "environment": environment}
        config_file = scratch / "browser.json"
        config_file.write_text(json.dumps(config))
        node = shutil.which("node") or str(root / ".tools/node/bin/node")
        browser = subprocess.check_output([node, str(root / "scripts/verify_wheel.cjs"), str(config_file)], text=True)
        report["browser"] = json.loads(browser.strip().splitlines()[-1])
        listing = subprocess.check_output([str(venv / "bin/jupyter"), "labextension", "list"], env=environment, text=True, stderr=subprocess.STDOUT)
        assert "fitsview-jupyter" in listing and "enabled" in listing
        report["extension_detected"] = True
        server = json.loads(info_file.read_text())
        headers = {"Authorization": "token " + server.get("token", "")}
        sessions = json.load(urllib.request.urlopen(urllib.request.Request(server["url"] + "api/sessions", headers=headers)))
        for session in sessions:
            request = urllib.request.Request(server["url"] + "api/sessions/" + session["id"], method="DELETE", headers=headers)
            urllib.request.urlopen(request).close()
    finally:
        if process and process.poll() is None:
            process.terminate()
            try: process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait()
        if runtime:
            binary = Path(f"/proc/{runtime['pid']}/exe")
            if binary.exists() and venv in binary.resolve().parents:
                os.kill(runtime["pid"], signal.SIGTERM)
    subprocess.run([python, "-m", "pip", "uninstall", "-y", "fitsview-jupyter"], env=environment, check=True)
    assert not (venv / "bin/fitsview").exists()
    assert not (venv / "share/jupyter/labextensions/fitsview-jupyter").exists()
    assert not (venv / "etc/jupyter/jupyter_server_config.d/fitsview-jupyter.json").exists()
    subprocess.run([python, "-c", "import importlib.util; assert importlib.util.find_spec('fitsview_jupyter') is None"], env=environment, cwd=data, check=True)
    report.update(uninstall_clean=True, passed=True)

(root / "reports/pip_validation.json").write_text(json.dumps(report, indent=2))
print(json.dumps(report))
