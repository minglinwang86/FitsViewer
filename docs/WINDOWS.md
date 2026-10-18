# Windows / Windows 桌面版

## Install / 安装

Use the x86_64 Setup EXE on Windows 10/11 with usable OpenGL. Install for the current user; administrator rights are not required. The native window opens FITS locally without Python, Jupyter or a server port. Windows ARM and older Windows versions are not validated.

在 Windows 10/11 x86_64 上运行 Setup EXE，显示驱动需支持 OpenGL。采用当前用户安装，无需管理员权限。独立窗口直接读取本地 FITS，无需 Python、Jupyter 或服务端口。Windows ARM 和更早的系统版本尚未验证。

Setup offers two independent choices:

- **Python / Conda environment:** leave “No Python association” for desktop only, or select an existing `python.exe`, venv or Conda environment folder. Python 3.10+ is required. Setup installs the Windows wheel into that environment and enables the Jupyter extensions. Allow dependency installation if JupyterLab or its integration dependencies are missing. Downloads use pip's configured package index. JupyterLab's supported range is `>=4.4,<5`; 5.x needs compatibility work and validation.
- **Default FITS viewer:** optionally associate `.fits`, `.fit`, `.fts` and `.fz`. Windows may require confirmation in **Settings > Apps > Default apps** when another application has a protected default. Setup offers to open that page. Generic `.gz` files retain their current association; compressed FITS can be opened from Browse or drag and drop.

安装时有两个独立选项：可指定已有 Python/Conda 环境并安装 Jupyter 集成，也可单独选择是否设为默认 FITS 查看器。若不关联 Python，则只安装独立桌面。JupyterLab 当前要求 `>=4.4,<5`。若 Windows 已保护其他应用的默认关联，需要在系统“默认应用”页面手动确认；程序不改写受保护的 UserChoice。普通 `.gz` 文件关联保持原样，FITS.gz 可通过浏览按钮或拖放打开。

Restart JupyterLab from the selected environment and refresh the browser after installation. Double-click FITS in its file browser. Notebook kernels in a separate environment also need the package to import `show()`.

An existing compatible JupyterLab is reused. Enabling dependency installation can update an older version to the supported 4.x range. Setup and `fitsviewer install` reject an existing 5.x environment before installing, rather than downgrading it. Direct pip commands follow pip's dependency resolver; choose a compatible environment before installing the wheel.

已有兼容版本会直接复用。勾选安装依赖可将旧版更新到支持的 4.x；安装器和 `fitsviewer install` 会提前拒绝已有 5.x 的环境，避免自动降级。直接运行 pip 则由 pip 解析依赖，应先选择兼容环境。

安装后，从所选环境重启 JupyterLab 并刷新浏览器，即可双击 FITS。Notebook 内核若使用另一环境，该环境也需安装包才能导入 `show()`。

The portable ZIP can be extracted to a writable folder. Start `fitsviewer.exe`, choose **Browse**, press **Ctrl+O**, or drop a FITS file onto its window. Portable mode does not automatically register defaults or Python environments.

免安装 ZIP 解压后运行 `fitsviewer.exe`，通过 **Browse / Ctrl+O** 或拖放打开。免安装模式不会自动注册文件关联或 Python 环境。

## Commands / 命令

PowerShell examples use generic paths. Run commands in an activated environment when using the pip entry point.

```powershell
python -m pip install ".\fitsviewer-0.1.6-py3-none-win_amd64.whl[jupyter]"
fitsviewer --native 'C:\data\image.fits'
fitsviewer 'C:\data\image.fits' --no-browser
fitsviewer defaults status
fitsviewer defaults set --jupyter yes --desktop no
fitsviewer desktop install
fitsviewer desktop remove
```

The pip `fitsviewer FILE` command uses Jupyter/browser mode. `--native` opens the desktop. Starting the desktop EXE directly uses native mode by default. After portable/Setup installation, equivalent association commands need no Python:

```powershell
& 'C:\path\to\FitsViewer\fitsviewer.exe' --desktop-default status
& 'C:\path\to\FitsViewer\fitsviewer.exe' --desktop-default yes
& 'C:\path\to\FitsViewer\fitsviewer.exe' --desktop-default no
& 'C:\path\to\FitsViewer\fitsviewer.exe' --desktop-default settings
```

Silent Setup accepts standard Inno switches plus explicit environment options:

```powershell
.\FitsViewer-0.1.6-windows-x86_64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TASKS="" /PYTHON="C:\path\to\env\Scripts\python.exe" /WITHJUPYTER=no /JUPYTERDEFAULT=yes
.\FitsViewer-0.1.6-windows-x86_64-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TASKS="associate" /CONDAENV="C:\path\to\conda\envs\astro" /WITHJUPYTER=yes /JUPYTERDEFAULT=yes
```

Omit `/PYTHON` and `/CONDAENV` for desktop only. `/WITHJUPYTER=no` requires a compatible JupyterLab, jupyter-server-proxy and IPython already installed. Default selection may still require Windows Settings confirmation; silent Setup cannot bypass it.

命令行静默安装仍支持 Python/Conda 环境和默认查看器选项；省略环境参数即只安装桌面。`/WITHJUPYTER=no` 要求所选环境已具备依赖。Windows 系统确认不会被静默安装绕过。

Jupyter integration failures are recorded in `%LOCALAPPDATA%\FitsViewer\install-jupyter.log`. Native windows use no server port; `fitsviewer ports` shows configured and running ports for browser/Jupyter mode.

## Uninstall / 卸载

Windows Settings or the Start menu uninstall entry removes the desktop and its registrations, restoring previously recorded fallback FITS defaults. An optionally installed Jupyter package stays in the selected environment so it remains usable independently. To remove that integration too, activate that environment and run:

```powershell
fitsviewer desktop remove
python -m pip uninstall fitsviewer
```

桌面卸载会清理自身关联并恢复已记录的旧设置。可选安装到 Python/Conda 的 Jupyter 包保留在所选环境中；如需一并删除，请在该环境执行上述命令。

## Build / 构建

Cross-build the native executable with a MinGW x86_64 compiler and the `x86_64-pc-windows-gnu` Rust target. The pinned toolchain and web build dependencies are listed in `DEPENDENCIES.txt`. From the Linux development environment prepared by `scripts/bootstrap.sh`:

```bash
source scripts/env.sh
bash scripts/build_windows.sh
```

Copy the Windows wheel and source to Windows. With Python 3.11+ and Inno Setup 6.7.3 installed, assemble a new payload directory outside the checkout:

```powershell
python scripts/build_windows_installer.py --wheel .\dist\full\fitsviewer-0.1.6-py3-none-win_amd64.whl --payload C:\build\fitsviewer-payload --out C:\build\release --iscc 'C:\path\to\Inno Setup 6\ISCC.exe'
```

The output includes a portable ZIP and Setup EXE. Inno Setup's compiler is not redistributed. Its installer engine retains upstream notices; see `windows/INNO-NOTICE.txt`. Release assets are not code signed.
