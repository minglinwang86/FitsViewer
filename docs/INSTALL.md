# pip 安装与 JupyterLab 默认打开 FITS

安装包包含 Rust 程序、WASM 网页、JupyterLab 预编译扩展和 Notebook 内嵌接口。使用 wheel 不需要 Rust、Node.js，也不需要执行 `jupyter lab build`。

当前提供 Linux x86_64 wheel（glibc 2.28 及以上）和 Windows x64 wheel、Setup 安装器、免安装 ZIP，下载链接和逐步命令见 [当前版本安装说明](releases/v0.1.4-rc.2.md)。Linux 原生 `--native` 窗口另需 X11/OpenGL 系统库，浏览器入口不需要桌面显示环境。Jupyter 集成需要 Python 3.10+、JupyterLab 4.4 及以上且低于 5。

## 安装到运行 JupyterLab 的环境

先激活实际启动 JupyterLab 的 Conda 或 venv 环境，然后安装下载的 wheel：

```bash
python -m pip install --upgrade "https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.4-rc.2/fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl"
fitsviewer defaults set --jupyter yes --desktop no
```

安装完成后重新启动该环境中的 JupyterLab，并刷新浏览器。扩展会自动注册 `.fits`、`.fit`、`.fts`、对应 gzip 文件及主要大写后缀。双击 FITS 文件就会默认使用 FitsViewer，支持缩放、scale 切换和右键亮度/对比度调节。`.fz` 会识别为 FITS，但当前内核仍不支持其压缩图像格式。

已有 FitsViewer 的 Linux 环境也可直接运行 `fitsviewer install --python /目标环境/bin/python` 或 `fitsviewer install --conda-env 环境名`。端口管理及这些命令的完整参数见 [基础命令](COMMANDS.md)。

`fitsviewer install` 安装成功后会用英文显示目标安装环境、端口配置、JupyterLab 是否绑定，并给出使用目标 Python 的绑定命令。端口配置与实际运行端口分别标明；扩展被禁用时显示原因。直接 `pip install` 使用 pip 自带的安装输出。

后端跟随当前 Jupyter 的文件根目录启动。Jupyter 的 URL 前缀也会用于查看器接口。

确认前后端扩展：

```bash
jupyter labextension list
jupyter server extension list
```

前端应显示 `fitsviewer` 已启用，后端应显示 `fitsviewer` 和 `jupyter_server_proxy` 已启用。若 FITS 的默认打开方式曾被用户改过，在文件列表中右键文件，选择 **Open With → FitsViewer**；再检查 JupyterLab 的默认查看器设置。若另一扩展也声明 FITS 为默认类型，需要选择一次所需查看器。

## Notebook 内嵌

```python
from fitsviewer import show
show("image.fits", height=650)
```

路径相对于 Jupyter 文件根目录。Notebook 内核与 JupyterLab 服务若处于不同 Python 环境，服务环境必须安装完整包以启用文件查看器，内核环境也要能导入 `fitsviewer` 才能调用 `show()`。仅在其他内核里执行 `%pip install` 不会给 JupyterLab 服务注册扩展。URL 前缀未由内核继承时，可显式传入 `base_url`。

## 命令行与远程访问

```bash
fitsviewer image.fits
fitsviewer --native image.fits
```

第一条命令优先连接同一 Python 环境中正在运行的 Jupyter 服务；没有服务时会启动独立查看器，并输出访问链接。第二条使用原生 Linux 窗口，需要已配置的显示环境。

SSH 端口转发方式保持不变。若本地浏览器端口与服务端 Jupyter 端口不同，可在启动 Jupyter 前设置 `FITSVIEWER_BROWSER_ORIGIN`，使命令行输出本地可用的链接，例如：

```bash
export FITSVIEWER_BROWSER_ORIGIN=http://127.0.0.1:18082
jupyter lab --no-browser --ip=127.0.0.1 --port=8082
```

## 发布和卸载

GitHub Releases 可以上传构建好的 wheel，用户下载后直接 `pip install`。当前尚未发布到 PyPI，因此 `pip install fitsviewer` 不能作为已发布软件的安装命令。将来发布 PyPI 后，同名命令即可使用。

```bash
python -m pip uninstall fitsviewer
```

卸载会删除程序入口和已安装的扩展资源。用户缓存保留在 `XDG_CACHE_HOME/fitsviewer` 或 `~/.cache/fitsviewer`，其中不复制 Jupyter 登录 token。卸载后重新启动 JupyterLab。

## 维护者构建

先按照 [构建说明](BUILDING.md) 准备开发环境，再运行 `scripts/build.sh --repair` 构建资源并生成 wheel。构建不自动安装到当前环境。已有构建产物时，可独立打包，不安装到当前环境：

```bash
python scripts/build_wheel.py
python scripts/build_wheel.py --repair
```

`--repair` 需要 `auditwheel` 和 `patchelf`，会检查 ELF 动态库兼容性并生成 manylinux wheel。平台标签由实际产物决定，不把包含 Linux 程序的包标记成通用 `py3-none-any`。

用隔离环境验证安装、默认打开、内嵌、命令行和卸载：

```bash
python scripts/verify_wheel.py dist/fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl
```

参考：[JupyterLab 预编译扩展分发](https://jupyterlab.readthedocs.io/en/stable/extension/extension_dev.html#distributing-a-prebuilt-extension)、[Jupyter Server Proxy 的 Python 包注册](https://jupyter-server-proxy.readthedocs.io/en/latest/server-process.html#specifying-config-from-python-packages)。
