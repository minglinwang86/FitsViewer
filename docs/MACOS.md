# macOS Full / macOS 全功能版

The macOS portable ZIP contains a native `FitsViewer.app` with Minimal and PRO,
plus a terminal launcher. Choose **arm64** for Apple Silicon or **x86_64** for Intel.
The application icon is included; FITS file icons and default associations are not changed.

macOS ZIP 包含原生 `FitsViewer.app`（极简及 PRO）和终端启动器。Apple Silicon 选择
**arm64**，Intel 选择 **x86_64**。包含应用图标，不修改 FITS 文件图标或默认打开方式。

| Processor / 处理器 | Download / 下载 |
|---|---|
| Apple Silicon | [macOS arm64 ZIP](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.6/FitsViewer-0.1.6-macos-arm64-portable.zip) |
| Intel | [macOS x86_64 ZIP](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.6/FitsViewer-0.1.6-macos-x86_64-portable.zip) |

## Open the application / 打开程序

1. Download the ZIP for your processor and compare its SHA-256 with the release's
   [SHA256SUMS](https://github.com/minglinwang86/FitsViewer/releases/download/v0.1.6/SHA256SUMS).
2. Extract the archive. Open `FitsViewer.app`, or copy it to `~/Applications` first.
3. Use **Open** inside the viewer to choose an image or spectrum.

下载后核对校验值并解压。直接打开 `FitsViewer.app`，也可先复制到 `~/Applications`。
通过程序中的“打开”选择文件。此版本不注册 Finder 双击 FITS 的关联。

This package uses an **ad-hoc signature**, without Developer ID signing or Apple
notarization. macOS may block the first launch. If you have checked the source and
checksum and choose to allow this app, Apple's documented route is to attempt opening
it, then use **System Settings → Privacy & Security → Open Anyway** for this application.
See [Apple's first-launch guidance](https://support.apple.com/en-us/102445).

本版本仅采用临时签名，未获得 Developer ID 签名和 Apple 公证。首次打开可能被
系统拦截。核实来源及校验值并决定使用后，可按 Apple 指引，在尝试打开后进入
“系统设置 → 隐私与安全 → 仍要打开”，单独允许该程序。

## Terminal and external Python / 终端与外部 Python

From the extracted directory / 在解压目录中执行：

```bash
./fitsviewer "/path/to/image.fits"
./fitsviewer --build-info
./fitsviewer "/path/to/image.fits" --inspect-json
./fitsviewer "/path/to/image.fits" --python "/path/to/science-env/bin/python"
./fver --help
./fver helper --url http://127.0.0.1:18877 --target science
```

Basic desktop viewing needs no Python interpreter. Scientific operations call the
selected external Python/Conda environment. NumPy, Astropy, Specutils, scientific
catalogues and TeX are not bundled. After moving only the `.app`, its equivalent
terminal entry is `FitsViewer.app/Contents/MacOS/FitsViewer`.

From 0.1.6, the portable directory also includes `fver`, using four shared Python
source modules and your external Python 3.10+. Set `FITSVIEWER_PYTHON` to select
that interpreter. The helper requires an explicit pairing code and local SSH
forward; it does not install Python or Jupyter. See [SSH viewing](REMOTE_VIEWING.md).
The wrapper avoids bytecode writes inside the signed application resources.

The app's `Contents/Resources` includes the shared notices plus
`MACOS_THIRD_PARTY_NOTICES.md` and a target-specific license inventory under
`licenses/`. These retain the original license texts and upstream attribution for
the native Mac dependencies.

基础桌面查看不要求 Python。科学功能调用指定的 Python/Conda 环境，依赖需在该环境
安装；包内不包含上述科学包、星表或 TeX。若只移动了 `.app`，终端入口为
`FitsViewer.app/Contents/MacOS/FitsViewer`。

## Jupyter and validation scope / Jupyter 与测试范围

This release provides a macOS **desktop ZIP**, not a macOS Python wheel or a Jupyter
installer. Included shared web resources do not register an extension in a local
Mac Jupyter environment. For Jupyter running on a Linux server, install the Linux
wheel there and connect from your Mac using the [SSH/browser instructions](INSTALL.md).

本次 macOS 发行物是桌面 ZIP，不是 Python wheel，也不会自动绑定本机 Jupyter。
可在 Linux 服务器安装 Linux wheel，再从 Mac 通过 SSH 和浏览器访问。

Native builds and automated checks run on macOS 15 for both processors. The compiler
targets macOS 11.0, but older systems are not certified. GUI startup probes are
reported separately from interactive GUI acceptance. See the
[actual validation record](releases/VALIDATION-0.1.6.md).

两种架构均在 macOS 15 环境原生构建和自动检查。编译目标为 11.0，但没有认证旧系统；
窗口启动探测与完整交互验收分别记录。科研测量结果仍需独立验证，不能直接用作论文数据。
