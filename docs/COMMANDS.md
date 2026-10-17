# FITSView 基础命令

以下命令在 Linux 上运行。pip 安装后的 `fitsview` 与项目内的 `bin/fitsview` 提供同一套命令，也可以使用 `python -m fitsview_jupyter`。

## 打开图像与查看状态

```bash
fitsview image.fits
fitsview image.fits --hdu 1
fitsview --native image.fits
fitsview --help
fitsview --version
fitsview status
fitsview ports
fitsview ports --json
```

`status` / `ports` 只检查已有服务，不启动查看器。输出包含当前 Python 环境、配置文件位置、Jupyter 端口、内部查看器端口、浏览器地址和下次启动的配置。输出不包含登录 token。

## 端口含义

| 层级 | 示例 | 选择方式 |
|---|---|---|
| SSH 入口 | 服务器 22 | SSH 服务的入口，由服务器配置 |
| Jupyter | Linux 回环地址 8082 | `fitsview jupyter start` 的端口，可改为 auto |
| 浏览器入口 | 客户端 18082 | SSH 本地转发到 Jupyter 8082 |
| Rust 内部代理 | auto | Jupyter Server Proxy 自动分配，无需单独转发 |
| 独立查看器 | 默认 8765 | 没有可连接 Jupyter 时使用，可设为 auto |

访问链路为：浏览器 → SSH 通道 → Jupyter → Rust 查看器。用 `fitsview ports` 查看实时值；自动分配的内部端口不是固定 SSH 转发目标。

## 保存和改变端口配置

```bash
fitsview ports set --jupyter 8084 --browser 18084
fitsview ports set --viewer 8766
fitsview ports set --viewer auto --proxy auto
fitsview ports set --ssh-host user@server --ssh-port 22
```

- `--jupyter`：由 `fitsview jupyter start` 使用的服务端端口；`auto` 先复用已连接的服务，否则选择空闲端口。
- `--browser`：客户端浏览器入口端口；`same` 表示与 Jupyter 端口相同。
- `--viewer`：独立 Rust 服务的端口；`auto` 由操作系统选择。
- `--proxy`：Jupyter 内部 Rust 服务端口；默认 `auto`，通常无需改动。固定此端口的配置要在下次启动 Jupyter 时生效。
- `--ssh-port` / `--ssh-host`：生成 SSH 命令时使用的入口；请填写客户端实际可以访问的服务器地址和 SSH 端口。

普通安装的初始配置为：Jupyter `8082`、浏览器 `same`、独立服务 `8765`、内部代理 `auto`、SSH `22`。不同环境可以保存自己的配置。

配置按 Python 环境隔离，保存在 `~/.config/fitsview/<环境标识>.json`，支持 `XDG_CONFIG_HOME`；开发项目使用自己的 `.runtime/config.json`。`FITSVIEW_CONFIG` 可指定其他配置文件。

配置修改用于后续启动，不会关闭已有 Jupyter、改变已监听的端口或改写正在运行的 SSH 通道。浏览器入口地址如果已由 `FITSVIEW_BROWSER_ORIGIN` 环境变量固定，须在新的 Jupyter 启动中更新它。当前运行值与保存值会分别显示。

## 启动 Jupyter 和生成 SSH 映射

```bash
fitsview jupyter start --port 8084 --browser-port 18084 --root /data
fitsview jupyter url
fitsview tunnel --ssh-host user@server --ssh-port 22
```

`jupyter start` 使用当前 Python 环境中的 JupyterLab；匹配端口和文件根目录的已连接服务会被复用。端口被其他程序占用时会报错，可显式使用 `--port auto`。选择另一端口会启动另一个服务，已有服务继续运行。`jupyter url` 输出带 token 的登录链接，应仅供自己使用。

`tunnel` 只生成命令。把输出的 SSH 命令复制到**浏览器所在的电脑**执行，保持终端运行，然后访问输出地址。例如在服务端生成命令：

```bash
fitsview tunnel --ssh-host user@server.example.com --ssh-port 22 --local-port 18082 --remote-port 8082
```

把生成的 SSH 命令复制到浏览器所在电脑执行。服务器地址必须能从该电脑访问；服务器不能修改客户端已有 SSH 进程。

## 安装到另一个已有 Jupyter 的环境

```bash
fitsview install --python /path/to/env/bin/python --check
fitsview install --python /path/to/env/bin/python
fitsview install --prefix /path/to/env
fitsview install --conda-env astro
fitsview install --conda-env base --check
```

三种目标选择任选一种。目标需要 Python 3.10+、JupyterLab 4.4+ 且低于 5。`--check` 仅检查，不安装。若目标缺少 JupyterLab，可显式允许安装它：

```bash
fitsview install --python /path/to/env/bin/python --with-jupyter
```

命令通过**目标 Python 的 pip**安装完整查看器。普通 pip 安装版可从自身已安装的资源重建 wheel，因此原始下载文件已删除时也能安装到另一个环境；无需 Rust、Node.js 或 PyPI 已发布的包。开发项目优先使用 `dist/` 里已构建的 wheel。也可明确指定安装包：

```bash
fitsview install --prefix /path/to/env --wheel /path/to/fitsview.whl
```

安装后从目标环境重新启动 JupyterLab，就能默认双击打开 FITS。安装命令本身不会重启已有服务。Conda 无法从 PATH 找到时，使用 `--conda-exe /path/to/conda`；同名环境有歧义时改用完整 `--prefix`。

`fitsview install` 成功后返回英文提示，显示目标环境、Python、目标端口配置、已连接的运行端口，以及 JupyterLab 扩展绑定状态。例如：

```text
FITSView installation successful (version 0.1.4rc1).
Installation environment: /path/to/env
Python executable: /path/to/env/bin/python
JupyterLab port: 8082 (configured)
Browser access port: 8082 (configured; same as JupyterLab)
Standalone viewer port: 8765 (configured)
JupyterLab viewer port: auto (assigned when the viewer starts)
Running JupyterLab port: none detected in this environment
JupyterLab binding: Yes (extensions enabled in configuration)
```

`configured` 是保存的启动配置，不表示安装命令已启动监听服务。内部代理的 `auto` 端口在打开查看器时分配。绑定检查包含两个后端扩展与前端扩展；关闭扩展时显示 `No` 并列出原因。`Yes` 表示配置已启用，已有 JupyterLab 进程仍需重新启动以加载新安装的资源。

提示始终包含目标环境的绑定命令，路径含空格时会自动加引号：

```bash
/path/to/env/bin/python -m jupyter server extension enable --py fitsview_jupyter --sys-prefix
/path/to/env/bin/python -m jupyter server extension enable --py jupyter_server_proxy --sys-prefix
/path/to/env/bin/python -m jupyter labextension enable fitsview-jupyter fitsview-jupyter:viewer --level=sys_prefix
```

这些命令启用目标环境中的扩展。如果用户配置或正则规则仍显式禁用扩展，需在对应配置中解除禁用。直接 `pip install` 保留 pip 自带的标准输出；上述详细提示由 `fitsview install` 提供。

## `.tar` 的 UTF-8 报错

Jupyter 的文本编辑器无法直接打开 tar 二进制归档。FITSView 注册的是 FITS 图像，不是 tar 文件。应先解包，再打开其中的 `.fits` / `.fits.gz`。

通用解包示例：

```bash
tar --keep-old-files -xf images.tar -C ./data
```

目标目录需已存在。已解包时直接打开其中的图像；`--keep-old-files` 避免覆盖已有文件。

## 交互安装、pip 与 Git

`fitsview install` 在终端中提供当前 Python、指定 Python 路径、Conda 环境三个选项，再询问 Jupyter 和桌面是否设为默认。脚本或 SSH 自动安装可显式传参，不依赖图形桌面：

```bash
fitsview install --conda-env astro --default-viewer yes --desktop-default no
fitsview install --python /path/to/env/bin/python --default-viewer no --desktop-default no
```

pip 的目标环境通过 Python 或 Conda 选择，应用配置由程序命令完成：

```bash
conda run -n astro python -m pip install ./fitsview_jupyter-0.1.4rc1-py3-none-manylinux_2_28_x86_64.whl
conda run -n astro fitsview defaults set --jupyter yes --desktop no
```

Git 源码安装入口为 `scripts/install.py --from-source`，同时接受上述目标环境和默认查看器参数。构建 Python 需 3.11+，目标运行 Python 可为 3.10+；完整步骤见 [构建说明](BUILDING.md)。

## 默认查看器与 Linux 桌面

```bash
fitsview defaults status
fitsview defaults set --jupyter yes --desktop no
fitsview defaults set --jupyter no
fitsview desktop install
fitsview desktop install --no-default
fitsview defaults set --desktop no
fitsview desktop remove
```

Jupyter 默认设置按 Python 环境保存；修改后刷新 JupyterLab，仍可通过 Open With 选择 FITSView。扩展是否绑定与是否为默认查看器是两个独立状态。

桌面关联按 Linux 用户保存，适用于文件管理器、X2Go 桌面或本机 X11/XWayland 桌面，启动对应环境的原生 `--native` 窗口。需要 `xdg-utils`、`shared-mime-info` 和可用 OpenGL；可安装 `desktop-file-utils` 刷新桌面缓存。只有最后一次注册的环境会成为该用户的 FITSView 桌面入口。

取消桌面默认设置会恢复之前记录的查看器；此前没有默认项时由桌面系统选择。卸载或删除对应环境前，先运行 `fitsview desktop remove` 清理入口。X2Go 中仍需具备可用的显示/GLX 环境；必要时可尝试：

```bash
LIBGL_ALWAYS_SOFTWARE=1 fitsview --native image.fits
```
