# Install FitsViewer 0.1.5rc1 / 安装指南

The Full distribution includes Minimal and PRO interfaces, the native desktop, browser viewer, prebuilt JupyterLab extension and Notebook API. No Rust/Node toolchain or `jupyter lab build` is needed when installing a wheel. Scientific Python packages remain external.

全功能包包含极简／PRO、原生桌面、浏览器查看器、预编译 JupyterLab 扩展和 Notebook 接口。安装 wheel 不需要 Rust、Node 或 `jupyter lab build`，科学包仍使用所选外部环境。

## Download / 下载

[Release assets and checksums](https://github.com/minglinwang86/FitsViewer/releases/tag/v0.1.5-rc.1)

| Platform / 平台 | File / 文件 |
|---|---|
| GNU/Linux x86_64, glibc 2.28+ | [Linux wheel](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.5-rc.1/fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl) |
| Windows x86_64 desktop | [Setup EXE](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.5-rc.1/FitsViewer-0.1.5rc1-windows-x86_64-setup.exe) · [Portable ZIP](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.5-rc.1/FitsViewer-0.1.5rc1-windows-x86_64-portable.zip) |
| Windows Python/Jupyter | [Windows wheel](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.5-rc.1/fitsviewer-0.1.5rc1-py3-none-win_amd64.whl) |
| macOS Apple Silicon / Intel desktop | [arm64 ZIP](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.5-rc.1/FitsViewer-0.1.5rc1-macos-arm64-portable.zip) · [x86_64 ZIP](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.5-rc.1/FitsViewer-0.1.5rc1-macos-x86_64-portable.zip) |

The project is not on PyPI; use these wheel files, not an unrelated similarly named package. Select Python 3.10+ and JupyterLab 4.4–4.x. The environment installer rejects JupyterLab 5 rather than silently downgrading it. Direct pip commands use pip's resolver, so choose the intended environment first.

本项目未发布 PyPI，请使用上述 wheel。选择 Python 3.10+、JupyterLab 4.4–4.x 的环境。环境安装器会拒绝 5.x，不自动降级；直接 pip 命令则由 pip 解析依赖，应先选好目标环境。

## Linux pip and Conda / Linux 安装与环境选择

Activate the environment that starts the Jupyter server. Install the downloaded wheel with `[jupyter]`, which adds Lab, server-proxy and IPython integration dependencies. Installing a plain wheel only installs FitsViewer.

激活真正启动 Jupyter 服务的环境，使用 `[jupyter]` 安装集成依赖。普通 wheel 不会自动安装 Jupyter 软件栈。

```bash
python -m pip install --upgrade "./fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl[jupyter]"
fitsviewer defaults set --jupyter yes --desktop no
fitsviewer jupyter start --root "$HOME/data" --port 8082
```

The root directory must exist. Restart an already running Jupyter service after installing, then refresh the browser. Double-click FITS, FITS.gz and supported FZ files. Text spectra retain their normal editor: select **Open With → FitsViewer**.

数据目录须存在。安装后重启已有 Jupyter 服务并刷新浏览器，FITS、FITS.gz、支持的 FZ 可双击打开；普通文本光谱通过 **Open With → FitsViewer** 打开。

Install directly into an existing Conda environment without activating it:

```bash
conda run -n astro python -m pip install "./fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl[jupyter]"
conda run -n astro fitsviewer defaults set --jupyter yes --desktop no
```

Once installed, FitsViewer can copy its complete package into another existing environment:

```bash
fitsviewer install --python /path/to/env/bin/python --check
fitsviewer install --python /path/to/env/bin/python --default-viewer yes --desktop-default no
fitsviewer install --conda-env astro --with-jupyter --default-viewer yes --desktop-default no
```

Without `--with-jupyter`, the installer requires a supported Lab and pins its existing version while adding missing integration dependencies. `--with-jupyter` permits installing/updating Lab within the supported range. `--no-deps` is for an already prepared environment. Interactive `fitsviewer install` asks for the Python/Conda target and default choices. Success output reports the environment, configured/live ports, binding and repair commands in English.

不加 `--with-jupyter` 时要求目标已有兼容 Lab，并固定其版本补齐集成依赖；加该参数才允许安装或升级 Lab。`--no-deps` 适用于依赖已齐备的环境。交互安装会询问环境和默认关联，成功后用英文报告环境、端口、绑定状态及修复命令。

## SSH, desktop and ports / 远程、桌面和端口

On the server / 服务端：

```bash
fitsviewer jupyter start --root "$HOME/data" --port 8082 --browser-port 18082
```

On the client computer, including Windows or macOS / 本地电脑：

```bash
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 127.0.0.1:18082:127.0.0.1:8082 user@server.example.com
```

Open the Jupyter login URL using local port 18082. Keep tokens private. Only Jupyter needs forwarding; its internal viewer proxy chooses a separate automatic loopback port.

在登录链接中使用本地端口 18082；登录令牌须保密。仅转发 Jupyter 端口，内部查看器代理自动分配回环端口。

```bash
fitsviewer image.fits --no-browser
fitsviewer --native image.fits
fitsviewer ports
fitsviewer ports set --jupyter 8084 --browser 18084
fitsviewer desktop install
fitsviewer defaults set --jupyter yes --desktop no
```

Native Linux needs X11/XWayland and usable OpenGL. A configured X2Go session can provide a display, but actual forwarding is not certified by this release. `desktop install --no-default` adds only an Open With entry. Port changes apply to future starts and do not rewrite running SSH tunnels.

Linux 原生窗口需要显示服务和 OpenGL；X2Go 需单独配置，本版未认证实际转发。`desktop install --no-default` 只注册打开方式。端口修改用于下次启动，不会改写当前 SSH 隧道。

## Windows / Windows 安装

Use Setup for per-user installation or extract the portable ZIP. The native viewer works without Python. Setup independently offers Python/Conda binding and a default FITS viewer checkbox; enable Jupyter dependency installation if that environment lacks the integration packages. [Detailed Windows guide](WINDOWS.md).

Setup 可按用户安装，或解压 portable ZIP 直接运行。独立桌面无需 Python。安装界面分别提供 Python/Conda 绑定及默认 FITS 查看器选项；目标缺集成依赖时，勾选允许安装 Jupyter 依赖。

For an existing Windows Jupyter environment / 已有 Windows Jupyter 环境：

```powershell
python -m pip install --upgrade ".\fitsviewer-0.1.5rc1-py3-none-win_amd64.whl[jupyter]"
fitsviewer defaults set --jupyter yes --desktop no
```

The app, launcher and viewer tabs have the FitsViewer icon. Associated data files keep generic icons.

## macOS desktop / macOS 桌面

Extract the ZIP for your processor and open `FitsViewer.app`. Basic viewing needs
no Python; optional scientific tools use the selected external environment. The app
uses an ad-hoc signature without Apple notarization, so read the
[macOS first-launch and validation notes](MACOS.md) before opening it.

按处理器选择 ZIP，解压后打开 `FitsViewer.app`。基础查看无需 Python；科学功能使用
所选外部环境。应用采用临时签名，未做 Apple 公证，首次打开方式和测试边界见上方说明。
此 ZIP 不绑定 Mac 本机 Jupyter；Mac 浏览器仍可通过 SSH 访问 Linux Jupyter。

## Git source installation / Git 源码安装

```bash
git clone --branch v0.1.5-rc.1 https://github.com/minglinwang86/FitsViewer.git
cd FitsViewer
python3 scripts/install.py --from-source --build-python python3.11 --conda-env astro --with-jupyter --default-viewer yes --desktop-default no
```

This compiles the Full Linux package and needs [build prerequisites](BUILDING.md). To avoid compilation, use `scripts/install.py --wheel /path/to/downloaded.whl --python /path/to/env/bin/python --with-jupyter`. The source tarball is a source checkout, not a pip-installable sdist.

源码安装会编译全功能 Linux 包。使用 `--wheel` 可直接安装预编译包。源码 tar 包不是可直接 pip 安装的 sdist。

## Verify, Notebook and removal / 验证、内嵌和卸载

```bash
fitsviewer build-info
fitsviewer status
jupyter server extension list
jupyter labextension list
```

Enable the `fitsviewer` and `jupyter_server_proxy` server extensions and the `fitsviewer` Lab extension. For an explicitly disabled installation, run these in the target environment:

```bash
python -m jupyter server extension enable --py fitsviewer --sys-prefix
python -m jupyter server extension enable --py jupyter_server_proxy --sys-prefix
python -m jupyter labextension enable fitsviewer fitsviewer:viewer --level=sys_prefix
```

```python
from fitsviewer import show
show("data/image.fits", height=650)
```

Paths are relative to the server root. A separate kernel environment needs the importable package; installing in the kernel alone does not register the server extension. WCS/science tools default to the server's Python; use `--python` or `FITSVIEWER_PYTHON` to select another existing science environment.

路径相对于服务根目录；只在另一个 Notebook 内核安装，不会注册服务扩展。科学工具默认使用服务端 Python，可通过 `--python` 或 `FITSVIEWER_PYTHON` 指定已有科研环境。

Before uninstalling a registered desktop entry, run `fitsviewer desktop remove`. Then:

```bash
python -m pip uninstall fitsviewer
```

Restart Jupyter afterward. User preferences/cache are retained; observation files are not removed.
