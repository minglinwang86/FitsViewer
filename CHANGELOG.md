# Changelog / 版本记录

## 0.1.6 — Full / 全功能版

- Compact shared English/Chinese toolbars, horizontal PRO task buttons, standalone Settings/Header windows, and primary actions visible on dedicated tool pages.
- Clear all apertures, selected-aperture Delete/Backspace, shared display rotation with a directly visible Reset for images/regions/layers/picking, and an adjustable scale bar beside HDU.
- Catalogue identity overlays separated from local intensity candidates; Gaia defaults to a 3 arcsec validation limit and accepts legacy FITS `DD/MM/YY` observation dates without guessing the century.
- CDS catalogue presets and explicit nearby-observation searches across major archive interfaces; current-image spherical footprint filtering, 40%–150% area preference with partial fallback, bounded HiPS FITS cutouts and download/save/add actions.
- External-environment dependency inspection and opt-in missing-package installation, with View-role ownership, cancellation and retained installation results.
- Ordinary-SSH `fver` entry, independent browser service and optionally paired native helper; no Jupyter or X11 prerequisite, and no bundled browser runtime.
- A 20 MB decoded-packet threshold for complete transfer; larger supported images use bounded numeric preview/tile/pixel APIs while scientific operations retain original coordinates and values.
- File capabilities, one-use bootstrap/pairing tickets, distinct browser/helper/artifact scopes, stable source identities and bounded request/job ownership.
- Fixed real HTTP numeric-query extraction, streaming POST regression, late-entered PRO heartbeat ordering, cancellation during persistent copies, and air/vacuum rest-frame spectral measurement consistency; retained first-failure and correction evidence.
- Source/module guidance, reproducible checks, work logs and release-specific validation distinguish implementation, numeric tests, actual network/GUI tests and platform limits.

中文：统一工具页与错误详情、孔径与旋转、星表／附近观测、依赖管理和普通 SSH 查看；
修正旧式 FITS 日期、HTTP 协议与光谱空气／真空约定，保留科学数值及生命周期边界。

## 0.1.5rc1 — Full prerelease / 全功能预发布

- Shared external Apache-licensed CJK fallback, Unicode path opening and paired-quote handling.
- Fixed-height bottom panel with two rows; cursor values cannot change the image viewport height.
- One-click Auto display in Minimal/PRO images: bounded cached-sample limits, adaptive Linear/Sqrt/Asinh stretch and reset brightness/contrast, with no embedded scientific dependencies.
- RICE/GZIP/GZIP2 tile adapter around fitsrs, including correct integer widths, edge placement, floating dithering and lossless gzip fallback.
- FITS 1D, scalar/vector BINTABLE and UTF-8 text spectrum plots, with units, uncertainty conversion, original-sample readout, zoom and pan.
- Minimal/PRO modes with independent startup preference, English/Chinese UI, shared J2000/Gaia coordinates and on-demand external scientific tools.
- A spectrum-specific PRO workspace with two command rows, a resizable sidebar, reference-line groups and external candidate detection.
- Framed PRO pages with docking, movable/resizable floating groups, rail folding and remembered layouts; Minimal popups remember their own image/spectrum geometry without loading PRO.
- Explicit spectral uncertainty HDUs/types, original per-HDU headers and resolved photometry uncertainty provenance. Photometry keeps its physical unit visible.
- DSS plate coordinates and self-contained polynomial WCS cards are retained; Gaia/TAN overrides remove the old plate model before applying the replacement.
- Bounded document/role leases, result histories and temporary artifacts, with owned worker process trees and a Jupyter supervisor lifetime guard.
- Extended Unicode paths cover Windows scientific artifacts whose generated names exceed the ordinary MAX_PATH limit.
- Source-coordinate regions, continuum windows and masks; continuum/normalization, EW/pEW/net-flux measurements, smoothing and bounded Gaussian/Voigt model previews.
- Observed/rest/relativistic-velocity display axes, custom line references and an adjustable relative blackbody curve.
- Shared visibility/order/opacity layer controls for native-grid spectra and image stacks. Field-star relative brightness matching calls the selected external NumPy/SciPy/Astropy environment; it does not infer an absolute photometric zeropoint.
- No new runtime dependency stack; package growth is measured separately for code and the shared font.
- Native macOS Apple Silicon and Intel portable apps, built on macOS with a shared Full resource lock; external Python selection and application icons, without Finder file associations or Apple notarization.
- Platform acceptance and its limits are recorded in docs/releases/VALIDATION-0.1.5rc1.md.

中文：新增按需初始化的极简/PRO、中英文界面、共同坐标/Gaia工具、光谱专用双行工具栏与可调侧栏；
光谱选区、连续谱、测量、平滑、受限模型拟合、坐标轴和相对黑体参考；多谱/多图共用图层式显隐、
层序及透明度。场星匹配只调整相对显示比例，外部科学环境不内嵌。详见
[光谱分析](docs/SPECTRAL_ANALYSIS.md)与[图层说明](docs/IMAGE_LAYERS.md)。

另补充可停靠、浮动、组合和折叠的 PRO 页面及独立极简弹窗状态保存；明确选择光谱/测光误差
来源并显示物理单位；保留 DSS 底片坐标模型，并在坐标覆盖时清除旧模型。文档、任务、临时
文件和子进程各自有回收边界，Jupyter 查看器跟随其宿主退出。实际验收范围另有记录。

## 0.1.4rc2 — Linux / Windows candidate, 2026-10-17

- Product branding is FitsViewer; Python distribution, import and terminal command are all `fitsviewer`, with no legacy aliases.
- Windows EXE icon/version resources and matching taskbar/shortcut identity.
- Actual interface preview in both public READMEs using a bundled synthetic image.

- Windows native desktop, Browse/Ctrl+O and drag-and-drop opening, bundled icon and explicit image close.
- Optional Python/Conda environment and independent FITS association choices in a per-user installer.
- Windows wheel, environment discovery, process checks and Jupyter command portability.
- Previous Windows fallback defaults restored on removal; protected UserChoice is handled through Windows Settings.
- Clarified the JupyterLab `>=4.4,<5` support range and its independence from native viewing.

### Included Linux and Jupyter functionality

- English/Chinese public READMEs, portable examples, support boundaries and release notes.
- Primary MIT license and bundled dependency/font notices included in distributions.
- Python long description and prerelease metadata; strict wheel metadata validation.
- Portable build/bootstrap, explicit installation, locked QA dependencies and CI checks.
- Clean source archive and release checksums.
- Frontend shared dependency ranges aligned with the JupyterLab 4.4 support floor.
- Portable x86_64 wheel build with a glibc 2.28 baseline.
- Terminal installation choices for Python/Conda and Jupyter/desktop defaults, shared by pip and Git routes.
- User-level Linux FITS MIME association and native desktop launcher, with commands to restore previous defaults.
- Explicit WebRunner/texture cleanup before a Jupyter document is detached; disconnected path signals and stopped popup polling on close.
- Multi-file memory diagnostics with a plain-text Jupyter control, weak-reference checks and isolated process measurements.

中文：包含 Linux/Jupyter 查看功能、中英文说明、许可、包元数据、构建与验证入口。

## 0.1.3 — local build, 2026-10-16

- English target-environment installation summary: ports, JupyterLab binding and repair commands.
- Verified enabled/disabled binding states and paths containing spaces.

## 0.1.2 — local build, 2026-10-16

- Port/status inspection, saved settings, SSH command generation and managed Jupyter startup.
- Installation into Python/prefix/Conda targets, including self-repacked wheels.

## 0.1.1 and initial MVP — local builds, 2026-10-16

- fitsrs backend, shared native/WASM viewer, JupyterLab default opening and Notebook embedding.
- Zoom/pan, display stretches/limits and right-drag brightness/contrast.

Historical entries describe local artifacts, not earlier public releases.
