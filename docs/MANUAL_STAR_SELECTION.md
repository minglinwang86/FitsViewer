# Python / PyEC 人工选星接口（0.1.7 开发版）

FitsViewer 可供 Python 程序显示 FITS、等待用户选星并返回坐标。当前交互入口是浏览器或
Jupyter 中的共享查看器；本接口没有实现原生窗口 IPC 选星或完整 XPA 服务。

选点使用原始 HDU 的 `image` 像素坐标。`origin=0` 与 NumPy/Astropy 像素中心一致，
`origin=1` 对应 DS9 image 约定。它不执行质心拟合、星表识别、测光或饱和剔除。
PyEC 获得坐标后继续执行自己的科学处理。旋转、缩放和显示位深不改变返回的坐标系。

## 独立 Python 调用

在安装 Full wheel 的环境中保存为 `choose_stars.py`，运行
`python choose_stars.py image.fits`：

```python
from pathlib import Path
import sys
from fitsviewer.interactive import ViewerClient

image = Path(sys.argv[1]).resolve(strict=True)
with ViewerClient.start(image.parent) as client:
    selection = client.select_stars(image, origin=0, timeout=600)
    selection.open_browser()
    result = selection.wait()
    print(result.points)         # 不可变的 ((x, y), ...)；原图零起点
    print(result.source.hdu)     # 实际打开的 HDU
    print(result.source.identity)
```

左键或 **X** 添加点，**Q / 完成** 提交，**Esc / 取消** 退出；可撤回最近一点或清空。
中键拖动平移，滚轮缩放。超时、换图、HDU/文件版本变化、关闭窗口不会返回伪造的成功结果。

`ViewerClient.start(root, port=0, public_url=None, startup_timeout=30)` 只创建并管理自己的
loopback 服务；`close()` 或 `with` 退出会取消未结束会话并回收该子进程。
`ViewerClient.from_runtime(path=None)` 连接本人已有的普通服务，不隐式启动服务。

远端文件路径由服务端在已配置的数据根目录内解析。SSH 使用实际存在的端口转发和对应
`public_url`；远端进程不能直接控制本机浏览器。可显式访问 `selection.url`，在本人浏览器
打开该链接。普通服务链接的 fragment 含授权信息，页面会立即移除；不要分享或写入公开日志。

## 在 PyEC 中显式选择 FitsViewer

下面演示已在 Linux 验证的 PyEC 显示/选点调用序列。保存为 `pyec_choose.py`，在同时有
PyEC 与 FitsViewer 的环境运行 `python pyec_choose.py image.fits`。它只展示接口调用，
不会启动完整 seeing/PSF/测光流水线：

```python
from pathlib import Path
import sys
from ecsnoopy import snoopylib
from fitsviewer.interactive import ViewerClient
from fitsviewer.pyec import make_pyec_xparun

image = Path(sys.argv[1]).resolve(strict=True)
with ViewerClient.start(image.parent) as client:
    original = snoopylib.xparun
    snoopylib.xparun = make_pyec_xparun(client)
    try:
        viewer = snoopylib.xparun(False)
        viewer.display(str(image))
        viewer.xpa_label(1, 1, 'X: select a star; Q: finish')
        while True:
            fields = viewer.xpaget('iexam key coordinate image').decode().split()
            if fields[0] == 'q':
                break
            x, y = map(float, fields[1:])
            print(x, y)          # DS9 image 一起点坐标
            viewer.xpa_region([[x, y]], False)
    finally:
        snoopylib.xparun = original
```

在已有 PyEC 启动脚本中，可用同样的 `try/finally` 包住其交互调用，替换当前 Python
进程内的 `snoopylib.xparun`。磁盘上的 PyEC 源码和默认 DS9 后端不变；恢复后仍可使用 DS9。
同一个 adapter 后续 `display()` 会通过请求队列更新已打开的查看页。

| DS9/PyEC 调用 | 当前支持范围 |
| --- | --- |
| `display(path, hdu=None, pix=..., pix2=...)` | 显示服务端 FITS；显式 HDU；image 坐标反馈标记 |
| `xpaget('iexam key coordinate image')` | 阻塞返回一次 `x X Y`（bytes）；完成后返回 `q 0 0` |
| `xpaget('scale limits')` | 等待查看器确认后返回真实显示上下限 |
| `xpaset(...)` | `cmap grey/gray`、`cmap invert [yes/no]`、`scale linear/zscale/limits`、`zoom to fit`、`regions delete` |
| `xpa_region(...)` / `xpa_label(...)` | 圆/方框纯显示标记与短选星提示；不创建测光孔径 |

`q` 后的两个零只是旧调用格式的占位符，不可作为坐标使用。若 **Q** 在 PyEC 处理上一点时
到达，已提交点仍会依次返回；完成后的可选显示反馈不会中断该计算。

新 API 的 `wait()` 返回可编辑后的最终列表；DS9 风格调用则增量消费点。
撤回已被 PyEC 消费的点会抛出 `SelectionChanged`，要求调用者重启选星，不能假装撤销
已经发生的科学计算。

## 异常、认证与范围

- `SelectionCancelled`：用户、客户端或文档切换取消；不把部分点当作完成结果。
- `SelectionTimeout`：服务端会话到期或等待超时；客户端会请求取消。
- `SelectionChanged`：已消费点被撤回或修改。
- `SelectionError`：来源/HDU/版本变化、连接、认证或会话错误。
- `UnsupportedDS9Command`：未支持的 DS9 命令；physical/FK5/WCS、array、frame、catalog 等不会被静默当作 image 坐标执行。

已有 Jupyter 服务用 `ViewerClient(proxy_url, jupyter_token=...)`，传入 Jupyter 自己的
用户认证凭据。普通服务用 `ViewerClient(base_url, token=...)`；二者只能选择一种。
私有 Jupyter 后端密钥只用于代理内部，不交给浏览器，也不作为用户认证凭据。
当前 `/api/selection` 不接受隔离 `fver` broker 的文件能力凭据。

## 源码与验证入口

| 职责 | 文件 |
| --- | --- |
| Python 客户端、等待/取消、来源与坐标检查 | `jupyter/fitsviewer/interactive.py` |
| PyEC 窄适配；不导入或修改科学包 | `jupyter/fitsviewer/pyec.py` |
| 认证会话、来源版本、有界点数、单写者和到期 | `crates/server/src/selection.rs` |
| 原图选点、编号标记、键盘与状态 | `crates/viewer/src/selection_ui.rs` |
| 查看器接入与浏览器同步 | `crates/viewer/src/lib.rs`、`web/index.html` |
| 针对性契约检查 | `qa/test_interactive.py`、`qa/selection_bridge.mjs`、Rust `selection` / `selection_ui` tests |

显示配置经 `requested_view/config_revision` 下发；实际 `view.low/high` 及 `view_revision`
只在 UI 确认后更新。不得把请求的上下限冒充已应用数值。选点和来源使用独立会话，
不能复用测光孔径或把客户端一/零起点转换放到科学数组中。
