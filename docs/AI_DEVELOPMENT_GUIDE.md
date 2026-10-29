# AI development guide / AI 读取与修改指南

先读 [架构](ARCHITECTURE.md)，再沿一次具体操作追踪调用链。本文是编辑地图和约束，
不代替阅读函数体，也不承诺已列出全部内部函数。优先在本文件列出的职责边界内修改。
历史决策、UI 多轮修改与踩坑、完整状态流和交接规程见 [AgentWorkLog](../AgentWorkLog.md)。

## 编辑地图

| Task | Start here | Follow through |
|---|---|---|
| FITS/HDU、`.fz`、光谱读取 | `crates/core/src/reader.rs` | `compressed_reader.rs`, `spectral_reader.rs`, core tests |
| 图像 stretch/自动显示 | `crates/core/src/display.rs` | `viewer/src/lib.rs`, `controls.rs`, `ribbon.rs` |
| 顶部工具栏与路径框 | `crates/viewer/src/ribbon.rs` | `lib.rs` 的操作、语言和模式状态 |
| 控件/主题/弹出面板统一样式 | `crates/viewer/src/ui_style.rs` | `controls.rs::apply_theme`, `ribbon::align_controls`, `workspace_ui::information` |
| PRO 子页面与结果 | `crates/viewer/src/pro.rs` | selected tool module, `science_client.rs`, server bridge |
| 停靠/浮动/折叠布局 | `workspace_ui.rs` | controller UI bodies, Preferences; do not duplicate scientific state |
| 极简弹窗位置/折叠/恢复 | `window_layout.rs` | typed context keys, Preferences, popup wrappers; keep independent of PRO |
| 换图、关闭、临时结果引用 | `document_lifecycle.rs` | `science_client.rs`, host sessions/process/artifact ownership |
| 无图主HDU头、误差图测光 | `header_ui.rs`, `server/src/measurement.rs` | core original headers and explicit error kind/HDU |
| 多图/多谱/相对亮度 | `multiview.rs`, `image_layers.rs` | `web/python/fitsviewer_image_matching.py` |
| 光谱交互/分析 | `spectrum.rs`, `spectral_lines.rs`, `spectral_analysis.rs` | `fitsviewer_spectra.py`, `fitsviewer_spectral_analysis.py` |
| WCS/Gaia | `coordinates.rs` | `fitsviewer_wcs.py`, `fitsviewer_gaia.py`, shared detection/TAP |
| 真实星表识别/附近观测 | `pro_forms.rs`, `pro.rs` | `fitsviewer_catalogue_services.py`, `fitsviewer_archive_services.py`; retain coverage, service and product semantics |
| 外部环境与依赖安装 | `environment_ui.rs`, `server/src/environment.rs` | `fitsviewer_environment.py`, `science.rs`, View-role job lifetime |
| 显示旋转/比例尺/选点 | `image_transform.rs`, `image_display.rs`, `pro_points.rs` | regions/layers/WCS/viewport export; never rotate original arrays |
| HTTP 大图与精确像素 | `core/src/remote.rs`, `server/src/remote_data.rs` | `viewer/src/remote_image.rs`, browser fetch boundary; decoded-HDU threshold, caches, source versions |
| 原生异步大图与显示位深 | `server/src/native_images.rs`, `core/src/display_tile.rs` | Same metadata/preview/tile store; Minimal-only 16-bit samples, original PRO tiles and exact pixel requests |
| 同窗打开与全部退出 | `server/src/native_windows.rs`, `viewer/src/controls.rs` | `lib.rs`/`ribbon.rs`, `remote_native.rs`, `web/index.html`, `jupyter/src/index.ts`; acknowledge opens, separate local/remote file capabilities, preserve Jupyter |
| 普通 SSH 的独立查看 | `jupyter/fitsviewer/remote.py`, `server/src/remote_broker.rs` | `remote_native.rs`, `remote_sources.rs`, `web/remote.js`, `scripts/check_remote.py`; scopes and pairing never share master credentials |
| 孔径测光 | `regions_ui.rs`, `core/src/regions.rs` | bridge `measure`, optional `fitsviewer_measure.py` |
| Python/PyEC 人工选星 | `jupyter/fitsviewer/interactive.py`, `pyec.py` | `server/src/selection.rs`, `viewer/src/selection_ui.rs`, `web/index.html`; [接入说明](MANUAL_STAR_SELECTION.md)，来源/HDU/起点必须显式 |
| CLI/API/worker | `server/src/main.rs`, `science.rs` | `jupyter/fitsviewer/cli.py`, `scientific.py` |
| Lab 双击、关闭释放 | `jupyter/src/index.ts` | Python `server.py`, shared viewer `release_view` |
| Jupyter 退出与子进程回收 | `server/src/supervisor.rs` | `server.py` supervisor PID, HTTP shutdown, science process tree |
| 环境/端口/默认查看器 | `jupyter/fitsviewer/` | `install.py`, `management.py`, `defaults.py`, platform helpers |
| 模块与打包 | `config/build-modules.json` | both Cargo manifests, `build_profiles.py`, `build_wheel.py`, `hatch_build.py` |
| macOS 原生便携包 | `scripts/build_macos_full.py`, `macos/launcher.c` | `config/macos-full-assets.json`, `macos_licenses.py`, `verify_macos_full.py`; keep resource locks and native license coverage |
| 多语言文本、增加语言 | `web/locales/registry.json`, `web/locales/{code}.json` | `web/locales/README.md`, `crates/viewer/src/i18n.rs`, `web/locales/index.js`, locale key/placeholder/layout checks |
| 中文字库、符号显示 | `scripts/prepare_cjk_font.py`, `crates/viewer/src/ui_style.rs` | font license/subset coverage; prefer vector disclosure icons over unverified font glyphs |

上表的 viewer 文件位于 `crates/viewer/src/`，Python 科学适配器位于 `web/python/`。
顶层 `lib.rs` 负责调度，避免把新的完整 UI/科学工具继续堆进其每帧更新函数。
`web/pkg/`、Lab 预构建目录、`target/`、`_vendor/` 和 wheel 都是产物，不能手改代替源码。

## 建议的一次修改流程

1. 查 `git status --short` 和最近工作日志，保留用户及其他任务的未提交修改。
2. 用 `rg` 定位入口、状态写入和所有调用方；阅读相关分支、取消、关闭和错误处理。
3. 明确作用于基础还是 PRO，图像还是光谱，native/WASM 哪些入口；再做小范围修改。
4. 新 Python 科学适配器必须在模块 registry 中恰好归属一次；共享能力提取为单一实现。
5. 更新注册表内所有语言的翻译、接口/科学限制和相应检查记录。区分源码静态检查、编译、实际 GUI
   操作和科学数值验证；不得用“编译成功”代替科学正确性或截图审美验收。

## 模块边界

- `base` 不能导入 PRO Rust 类型或要求 PRO Python 文件存在。基础/Gaia共享工具放独立
  小模块，不能为了 Mini 在构建时 AST 删除函数、复制改写科学算法。
- `desktop` 是可选宿主依赖。跨平台 `cfg` 要同时考虑平台和功能，例如 Windows Mini
  不应引用已经被 feature 排除的桌面模块。
- `jupyter` 是可选集成。`import fitsviewer`、CLI 端口和 build-info 不应主动导入
  Jupyter、IPython、Astropy。只有调用相关入口才加载外部依赖。
- Full 的运行时极简模式与编译时 Mini 不是同一件事。前者延迟创建 PRO 状态；后者
  不含其 Rust 模块和 Python 适配器。不得只隐藏按钮后就声称减小了二进制功能集合。
- 当前 PRO 按整体功能组选择，不声称已经实现任意插件组合。新增可选大能力时再按
  依赖和状态边界拆组，并同步客户端、宿主、Python、资源清单和能力拒绝逻辑。

## UI 修改约束

复用统一控件高度、图标几何和间距；使用稳定 ID 与翻译 key。不要靠空文本、字形基线
或固定超长空白实现对齐。路径仅缩略显示副本，完整 Unicode 字符串必须保留用于编辑、
复制、请求和科学文件读写。PRO 图像与光谱工作区只显示适用于当前数据类型的工具。

共享颜色、圆角、边框和按钮高度集中于 `ui_style.rs`；菜单项保持无框菜单语义。
PRO 标题以整个边框为中心，不能只在扣除右侧按钮后的区域中居中。图像对照控件
优先复用任务选项行，窄窗口收进同行“比较”菜单，不预留备用空行；文件标签单独保留，
长文件名不能改变分组宽度。顶部面板行数变化后需要重绘，使上一帧的裁剪边界及时更新。
菜单内的选择器使用原生级联子菜单；egui 的普通 ComboBox 会替换父弹窗，不能直接嵌套。

辅助说明进入模块右侧“…”；运行、取消、错误、科学单位、结果有效性与不确定度状态应
可见。收起/停靠/浮动是视图状态，不应重置任务、释放仍在使用的科学结果或停止轮询。

真实视觉验收至少覆盖已维护的六种语言、Mini/PRO、图像/光谱、窄窗、长 Unicode 路径，以及展开的定标、
测光、光谱测量/处理/拟合/比较、空/运行/失败状态。逐张实看截图；不能只截图默认图层。
使用小型公开真实科学数据；合成样本只用于可控数值与边界测试。

品牌图标只用于程序、启动器、快捷方式和查看窗口/标签。不要为关联的 FITS、
光谱或压缩包文件指定品牌图标，包括 Jupyter 文件列表和各系统的文件关联。
Jupyter 的普通启动器卡片使用命令的 `LabIcon`，避免依赖仅用于内核卡片的图标参数。

## 科学与资源约束

物理像素单位、掩码、坐标原点、光谱单位/空气真空/红移约定必须显式。未知单位不猜测，
无误差模型不生成 SNR/正式显著性。相对场星亮度匹配不是绝对测光定标。显示参数不得
改写原始数组；异步结果必须校验当前文档、HDU、generation/revision。

不内嵌 Python、Astropy/Specutils、科学数据、TeX。外部包从用户选定环境按需调用。
不要增加全尺寸 CJK 字体或重复纹理/数据副本。关闭、取消和切换文件检查 job/owner、
Arc、WASM 和纹理资源；进程 RSS 不立即下降不等于仍持有对象，也不等于没有泄漏。

科研资源的归属、generation/epoch、租约、产物移交和进程退出边界见
[RESOURCE_LIFECYCLE.md](RESOURCE_LIFECYCLE.md)。不要将 worker 创建直接放回短命请求线程：
Linux 父线程退出也可能触发 parent-death signal；稳定创建线程必须存活到 worker 已回收。

远程图块的内部坐标仍是原图零基像素。不得把图块宽高当作原图宽高做孔径、WCS 或
图层配准。低分辨率预览只供绘制/抽样统计，精确读数调用原始 pixel 接口，测量调用
原始源文件。轮廓掩膜要检查相邻图块，不能把每个 256 像素块的边界画成物理轮廓。
0.1.7 的 16 位量化只用于极简模式的大图显示；PRO 必须请求原始位深图块。切换精度
必须清空纹理/图块并隔离迟到响应；不能将极简量化预览存入 PRO 多图缓存。内部 FITS
压缩按所需源压缩块解码，保留整块内的 dithering 序列，不能按显示图块重新播种。
修改 packet 阈值时同时检查元数据、存储类型及光谱坐标/误差数组；不得按 `.gz` 文件
大小判断传输路径。浏览器取消要覆盖响应体读取，不能收到 headers 后就移除取消器。

环境选项不是 PRO 工具页。依赖任务从启动到 status/cancel 都属于 `view`，不要隐式
取得 PRO 租约；收回另一个窗口的租约不能误杀它。安装结果要先交付/移交日志，再允许
空闲 worker 退出。静态 mock 通过不能证明真实 venv 的安装、符号链接或进程回收通过。

`fver` 修改必须检查 control / pairing / browser / helper / artifact 能力分离、一次性票据、
内外层生命周期字段一致、文件身份、替换源路径、迟到 ACK、队列满与取消。不要仅修新
图块 API 而保留可绕过预算的旧完整图像端点。普通 SSH 本身不能控制本机浏览器；首次
URL 与可选本机助手各有职责，真实 HTTPS 与 SSH 内的 HTTP 也要分别描述。

巡天检索的面积比例是“候选天区面积 / 当前图像天区面积”，不是巡天总面积或文件大小。
主列表优先 40%–150%，排除超过 150% 的产品；当前有界响应没有合格产品时才展示低于
40% 的部分结果。另算真实相交面积，不能把接触一个角落写成全覆盖。RA 跨零点、极区、
畸变 WCS、未知投影与 DataLink 都有独立边界；几何未知只放诊断。HiPS 重采样切图不是
原始曝光，不得混淆来源。新增 Python helper 必须同步模块资源清单。

下载与保存有两个阶段：Python 只写 owned cache；Rust 校验产物、文档和 PRO epoch 后
向原文件目录唯一临时路径复制，以不覆盖的原子链接提交，再加载新标签。不能把持久
文件注册为待清理缓存。取消按钮发送请求后应等待原保存结果，不能把取消 ACK 当作已
取消；提交成功后不按路径回滚，避免误删用户随后替换的文件。release 在串行请求锁前
标记文档/epoch 与取消位，防止长复制阻塞关闭，也要覆盖“关闭先于复制注册”的竞态。

布局工具键由调用方组合为 `operation:task`，即使没有子任务也带冒号，例如
`archive_search:`。模块默认值按冒号前的 operation 判断；保存的用户尺寸按完整键区分。
测试必须覆盖真实调用键、旧布局迁移和显式尺寸恢复。曾因只测试 `archive_search`
裸字符串而漏过列表默认高度未翻倍的问题；只改内层 ScrollArea 也不能突破外层高度。
Gaia 质量判定同时包含拟合 RMS、验证 RMS 和最大验证残差；改错误信息时应完整报告
三个判据，不能看到两个 RMS 通过就删除第三个科学质量检查。

## 验证与构建范围

用户要求“只改源码/准备架构/不要构建”时，**Cargo check/test 也会编译，不能运行**。
同样不要运行 npm build、wheel/安装器构建或启动新候选服务。可执行的轻量检查是：

```bash
python -B qa/test_build_profiles.py
python -B scripts/build_profiles.py --profile mini --plan
python -B scripts/build_profiles.py --profile full --plan
```

以上只检查模块选择/边界/默认不执行约定，不证明 Rust 类型、链接或 GUI 已通过。
Python 语法检查可用 AST；Rust 可用只输出到标准输出的 rustfmt 解析，不改源文件。

有明确构建/测试授权后，按实际修改选择现有测试；feature 矩阵应分别覆盖 viewer 的
default/no-default，以及 host 的 none/desktop/pro/desktop+pro，再验证配对的 WASM、
CLI、Jupyter 和 native。平台和 ABI 结论必须来自对应环境；不能从一个 Ubuntu 构建
推断所有 Linux/Windows/macOS 兼容。检查通过后无需无目的重复全量测试。

产物检查要核对 profile 与 hash，不混用历史 full WASM 和新 Mini host。构建日志、
源文件清单、失败记录和验收结果要保留。清理仅删除已核对、无活进程引用的中间产物，
先记录范围与摘要；保留当前源码、工作日志、真实数据和用户服务。公共文档使用通用路径，
不包含个人目录、连接凭据或服务 token。发布必须遵循当前用户明确授权。
