# Scientific tools / 科学工具

This describes the current Full source, including the 0.1.6 workflow changes.
[Historical validation](releases/VALIDATION-0.1.5rc1.md) applies to its own artifacts;
new source and online services require separate acceptance.
本文说明当前 Full 源码与 0.1.6 工作流；历史验收只覆盖对应产物，不能代替新版测试。

## Modes and language / 模式与语言

**Minimal** includes the basic image/spectrum viewer, display tools, themes,
palettes, colorbar, histogram, manual threshold overlay, circular aperture with
sky annulus, and the shared **Coordinates** panel. **Settings → Language** switches
English/简体中文 immediately and saves the preference.

**View → Auto display** provides one-click image limits, stretch and default
brightness/contrast in Minimal and PRO. It reuses the cached sample of at most
1000 finite physical values: robust background/spread and percentile tails choose
Linear, Sqrt or Asinh; a balanced signed image uses a linear range. Constant or
sparse images have explicit fallbacks. This is a starting point for inspection,
not a unique perceptual optimum or photometric calibration. Low/high remain
manually editable. No extra Python package, full-image copy or PRO state is needed.

The compact Office/CAD-style ribbon groups commands by task. Minimal has
**View / Measure / Coordinates**; PRO adds **Analysis / Catalogues / Export**.
Choose a tab to expand it; double-click a tab or use the collapse button to hide
the panels. Narrow windows wrap complete groups when needed. Photometry and
coordinates occupy their own task groups instead of separate permanent toolbar rows.
Chinese image task-group captions occupy the left gutter, with full captions on
hover. English omits these small captions. Buttons and dropdowns in each group
share their height and vertical alignment.
Image HDU selection sits on the command row in both modes. Mini also puts the
histogram/colorbar controls there. Display and zoom groups use their content width
in both modes, avoiding reserved empty blocks between modules.
Top rows use the same control height and 4 px gaps. Short paths keep **Open**
near the UI midpoint; the path field grows with its contents until the remaining
buttons need space. An unfocused overflowing path shows its right-hand end with
a leading ellipsis. Focus restores the full editable path; opening and copying
always use the original string.

**Settings → Default startup mode** chooses Minimal or PRO for new windows. It is
separate from the current mode: changing this setting alone does not create a PRO
workspace or start its worker. Explicit Notebook/URL/session modes take precedence.

Drag the highlighted left edge of the PRO side panel to resize it. Image and
spectrum workspaces remember their widths and collapsed states separately. Use
the panel's **>** button or the ribbon's **Work area** toggle to hide it; choose
**Work area** or a PRO tool to restore it. Collapsing preserves running jobs,
candidates and PRO state. Switching to Minimal releases PRO resources as before.
Widths are bounded to keep space for the main image or spectrum.
Image and spectrum PRO modules put explanatory notes in the **…** menu beside
the module selector. Each image/spectrum layer has its own frame and **…** menu
for reference/alignment details. Task progress, measurement results, validation
warnings and errors remain visible.

**极简**包含基础图像/光谱查看、显示工具、主题、色表、色标、直方图、手动阈值叠加、
圆孔径加背景环测光和共用的**坐标**面板。**设置 → 界面语言**即时切换中英文并保存选择。
图像工具栏中文小标题放在左侧竖排，悬停可查看完整标题；英文不显示这些小标题。
同组按钮和下拉框统一高度与垂直对齐，不增加工具栏行数。
图像极简与 PRO 的 HDU 选择移至命令行；极简还将直方图/色标放到该行。
两种模式的显示和缩放组按内容宽度排列，减少模块间固定空白。
顶部各行统一控件高度和 4 像素间距。短路径默认将**打开**按钮置于界面中部；
路径框随内容增长，在为右侧按钮保留空间后停止扩展。失去焦点时优先显示右侧文件名，
用省略号缩略左侧目录；点击编辑、复制和打开仍使用完整路径。
图像与光谱 PRO 模块的说明收进模块选项右侧的 **…** 菜单；每个图层独立加框，
参考图层和对齐状态说明收进图层的 **…** 菜单。任务进度、测量结果、校验警告和错误继续直接显示。
**查看 → 自动显示**在极简与 PRO 中一键设置显示范围、选择线性/平方根/反双曲正弦拉伸，
并恢复默认亮度/对比度。复用最多 1000 个有效物理像素的已有抽样，以背景/散布和分位数
尾部作轻量判断；双向正负信号、稀疏和常值图有单独回退。结果是方便检查的起点，可继续
手动修改，不是唯一视觉最优解或测光定标；不需要额外 Python 包、整图副本或 PRO 状态。

紧凑 Office/CAD 风格功能区按任务分组。极简提供**查看 / 测量 / 坐标**，PRO 另外提供
**分析 / 星表 / 导出**。点击标签展开，双击标签或点击收起按钮折叠；窄窗口按需对完整分组换行。
孔径测光和坐标不再各自常驻占用一行。

**设置 → 默认启动模式**可选择新窗口打开极简或 PRO。默认模式与当前模式独立，
仅修改默认值不会创建 PRO 工作区或启动其进程；Notebook、URL 和会话显式指定的模式优先。

拖动 PRO 侧栏高亮的左边界可调整宽度，图像与光谱分别记住宽度和收起状态。点击
侧栏 **>** 或功能区的**工作区**可收起；再次点击**工作区**或选择 PRO 工具可恢复。
收起只隐藏界面，保留任务、候选与 PRO 状态；切回极简才按原流程释放资源。
侧栏宽度有上限，为主图或光谱保留空间。

**PRO** initializes its workspace on selection. It adds star/trace detection,
advanced apertures and templates, catalogues/XMatch/SAMP/VO, multiple views,
reprojection/RGB and figure export. Switching back preserves the current image,
display parameters and apertures while releasing PRO tasks and rebuildable caches.
Gaia calibration has a separate owner and remains available in both modes.

Both modes offer the eight DS9 scale choices: Linear, Log, Power, Sqrt, Squared,
Asinh, Sinh and Histogram Equalization. Log/Power share an adjustable exponent
(default 1000). The existing endpoint-normalized Log/Asinh mappings are preserved;
the output is not a bit-for-bit reproduction of DS9's color lookup tables.
Histogram equalization uses a 4096-bin CDF of finite physical pixels within the
display limits; it is shared with the colorbar and reused during brightness/contrast
adjustments. Left or middle drag pans; right drag changes bias/contrast.

两种模式都提供与 DS9 对应的八种拉伸：线性、对数、幂函数、平方根、平方、反双曲正弦、
双曲正弦和直方图均衡。对数/幂函数共用可调指数（默认 1000）。保留原有端点归一化公式，
不承诺与 DS9 色表逐像素一致。直方图均衡仅统计显示上下限内的有限物理像素，使用固定
4096 分箱；图像与色条共用统计，亮度/对比度调整不重复扫描。左键或中键拖动平移，右键拖动调亮度/对比度。

**PRO** 在选择时才初始化工作区，增加找星/迹线检测、高级孔径与模板、星表/XMatch/SAMP/VO、
多图、重投影/RGB 和图片导出。返回极简保留当前图像、显示参数和孔径，释放 PRO 任务与可重建缓存；
Gaia 校准独立管理，在两种模式中使用同一个坐标面板。

## Existing Python environment / 使用已有 Python 环境

**Settings → Scientific Python environment** is a shared independent window in
Minimal and PRO. Inspect the actual Python executable/prefix, choose an existing
Python/Conda environment, then check its imports. This selects the external science
worker; the Jupyter server and Notebook kernels keep their own environments.

Missing, incompatible and import-failed packages are distinct states. Select missing
packages and explicitly install them in that environment. The installer first
resolves a pip dry-run plan, refuses replacements/upgrades/downgrades and existing
dependency conflicts, then installs the fixed plan. Logs, cancellation and the
result belong to that viewer. An active job or undelivered result blocks environment
switching. Pip must support dry-run/report; the viewer does not upgrade pip silently.

环境选择移至**设置 → 科学计算 Python 环境**，与 Jupyter 内核环境区分。先检查实际
导入，再勾选缺失包安装；已有包冲突、导入失败不能简单当作“未安装”。安装不暗中
替换已有科学环境中的包，日志和取消入口可见。外部依赖仍不进入程序发行包。

Basic FITS/FZ/gzip/spectrum viewing and Rust aperture sums do not require the
scientific Python stack. The tools below import packages from the selected
environment on demand. They are neither embedded nor installed automatically.

基础 FITS/FZ/gzip/光谱查看及 Rust 孔径计算无需 Python 科学栈；下列工具按需调用所选环境中
已有的包，程序不内嵌或自动安装这些依赖。

<a id="catalogues-and-nearby-observations"></a>

## Catalogue identities and nearby observations / 星表身份与附近观测

**Find stars** projects Gaia DR3 or SDSS DR17 primary-star catalogue positions through
the active FITS/Gaia WCS. It needs usable sky coordinates. **Local source candidates**
instead detects intensity peaks; it does not identify a peak as a catalogue object
or perform PSF photometry. Gaia calibration defaults to a 3-arcsec quality threshold,
retains independent validation, and never changes the source FITS header.

**CDS XMatch** offers Gaia DR3, 2MASS, Pan-STARRS1, SDSS DR16 and Gaia DR2. Choose a
coordinate table or reuse identified/imported positions; specify the matching
radius and all/nearest strategy. Multiple epochs or nearby alternatives can yield
ambiguous candidates. Local XMatch keeps rows local; CDS XMatch explicitly uploads
the selected coordinates/columns to CDS.

**Nearby observations** defaults to the current image's WCS footprint, including
an accepted Gaia overlay. It searches metadata first; a chosen product is downloaded
only on request. The main list prioritizes candidate sky areas of **40%–150%** of the
current image's sky area, with actual intersection checked separately. Products
over 150% are excluded. If this bounded response has no preferred product, smaller
partial products can appear. The displayed overlap percentage is the intersection
area divided by the current image's area; it is distinct from the area ratio.
Unverified geometry remains in diagnostics rather than the matching-product list.

| Preset | Scope |
| --- | --- |
| Aladin / CDS | Discover 2D FITS HiPS covering the current field; generate a bounded, resampled FITS cutout on request |
| SDSS DR17 | Calibrated ugriz survey frames |
| DES DR2 coadds / single exposures | Separate stacked and single-epoch queries |
| Legacy Surveys DR9 | DECaLS/BASS/MzLS products |
| NOIRLab calibrated archive | Calibrated multi-instrument images |
| MAST | HST/JWST and other missions, then observation product lists |
| ESO | Archive observations/products; access restrictions may apply |
| CADC | Multi-collection observations and DataLink products |
| IRSA | WISE, 2MASS, NEOWISE and supported Euclid collections |
| ALMA | Millimeter/submillimeter observations; image/cube products may be large |
| ASTRON | Radio surveys including LoTSS/LoLSS |

Where a preset has tested fallback endpoints, the adapter tries its bounded
sequence and retains earlier failures. A timeout or malformed response is not an
empty successful search. A record without a usable footprint remains unverified;
an inconsistent footprint is not offered as matching coverage. DataLink must be
resolved and a concrete product chosen. Searches do not automatically download
large data; cube/raw/bundle products may exceed viewer support or preparation limits.
Past availability checks do not guarantee current service uptime or field coverage.

**Generate FITS cutout and add** / **Download and add to viewer** validates the
response, saves it with a unique name beside the original FITS and opens it as an
additional viewer file. The original remains unchanged. If the original directory
cannot support an atomic save, the UI explains that it is opening a temporary
result instead. Downloads and decompression each have a 512 MiB limit. HiPS cutouts
are resampled from survey tiles; they are neither original exposures nor a promise
of flux-conserving scientific reprojection. Inspect provenance, masks, calibration
and access rights before scientific use.

Footprints use 32 sampled positions along the physical image boundary, spherical
areas and intersections, including RA wrap and polar fields. This is a sampled WCS
representation, not exact integration of arbitrary distortion. Supported discovery
fields have a radius up to 1.5°; complex or invalid geometry is rejected explicitly.

“找星”与“局部光源候选”用途不同；不能以亮点代替星表身份，也不能以巡天覆盖代替
某次曝光。网络检索只发送所选天区/查询，下载由用户明确选择产品。普通可信 Jupyter
可使用这些在线工具；`fver` 的文件能力不授予任意 URL 下载、环境更改或任意目录访问。
文件能力下的表格必须自包含，外部 STREAM/DTD/实体会被拒绝；合法产品链接仍可显示。

## External package reference / 外部包参考

| Tools / 功能 | External packages / 外部包 |
|---|---|
| J2000, sky coordinates, catalogues, sky templates / 天球坐标、星表、天球模板 | NumPy, Astropy; finite-width sky paths also need Shapely / 有限宽度天球路径另需 Shapely |
| Star detection, local XMatch, profiles, Gaia fit / 找星、本地匹配、剖面、Gaia 拟合 | NumPy, Astropy, SciPy |
| Gaia reference query / Gaia 参考星检索 | PyVO; optional Astroalign for pattern matching / 模式匹配可选 Astroalign |
| Straight trace detection / 直线检测 | SciPy, scikit-image, NumPy, Astropy |
| Spectral candidate emission/absorption / 光谱候选发射与吸收 | Specutils >=2,<3, Astropy, NumPy, SciPy; opt-in requirements-spectra.txt / 可选依赖清单 |
| Spectral continuum, measurements, smoothing, bounded profiles / 连续谱、特征测量、平滑、有界线形 | NumPy, SciPy in the selected environment; no embedded runtime / 调用所选环境，不内嵌运行时 |
| Field-star relative layer brightness / 图层场星相对亮度 | NumPy, SciPy, Astropy; no absolute zero point, PSF matching or bundled catalogue / 不推断绝对零点、不做 PSF 匹配、不内嵌星表 |
| Reprojection and RGB / 重投影与 RGB | NumPy, Astropy; RGB preview also uses Matplotlib / RGB 预览另用 Matplotlib |
| Figure and MathText export / 图片与数学标注导出 | Matplotlib, NumPy, Astropy for images / 图像还需 Astropy |
| SIA v1, SSA, TAP / 在线检索 | PyVO, Astropy |
| CDS XMatch | Astroquery, Astropy |
| SAMP | Astropy, an external Hub/client / 外部 Hub 与客户端 |

```bash
fitsviewer image.fits --python /path/to/env/bin/python
fitsviewer features --python /path/to/env/bin/python --json
fitsviewer science wcs --params wcs.json --python /path/to/env/bin/python
```

`FITSVIEWER_PYTHON` also selects an existing interpreter. Jupyter uses its server's
environment by default; a separate kernel environment does not change the server.
也可通过 `FITSVIEWER_PYTHON` 选择解释器。Jupyter 默认使用服务端环境；更换 Notebook 内核
不会同时更换 Jupyter 服务端解释器。缺包提示列出缺失包与安装命令；所选环境信息可通过 features 查看。

## Coordinates and Gaia / 坐标与 Gaia

Open **Coordinates** for one-based pixels, FK5 J2000 sexagesimal/degree readouts,
copy actions and **Recognize / calibrate with Gaia**. Existing WCS or RA/DEC and
pixel-scale metadata supply the initial field; optional approximate coordinates
and scale can be entered. This is a bounded field solution, not an all-sky blind
solver. The search radius is at most 1.5 degrees. No catalogue/index is bundled.

打开**坐标**可查看、复制从 1 开始的像素坐标及 FK5 J2000 坐标，并执行**一键识别 / 校准 Gaia 坐标**。
初始天区来自已有 WCS、RA/DEC 与像素角尺度，也可手动提供近似指向和尺度。
此功能是有限天区的匹配校准，非全天盲解；检索半径最多 1.5 度，不内嵌星表或索引。

Only the search field/query is sent to ESA Gaia TAP; images stay local. A local
reference table can replace the query. The TAN fit needs at least ten unique
matches and independent validation stars. The default RMS limit is 3 arcsec;
the panel exposes the configured limit, fitted and validation RMS. This is not
a universal accuracy guarantee. Proper motions are propagated when epoch and
motion data are usable; otherwise the result explains the retained epoch.

仅向 ESA Gaia TAP 发送检索天区/查询语句，图像保留在本地；也可使用本地参考表。
TAN 拟合至少需要十个唯一匹配和独立验证星。默认 RMS 门槛为 3 角秒，面板显示可调整门槛、
拟合 RMS 与独立验证 RMS；门槛并非通用精度承诺。有可用历元及自行时执行传播，否则结果说明采用的历元。

A validated solution is an in-memory WCS overlay used by coordinates, catalogue
overlays, sky view links/templates and figure export. Its checkbox restores the
original WCS. The source FITS header, physical pixels and calibration fields are
not rewritten. Save/download the JSON solution for provenance.

通过验证后，坐标解作为内存 WCS 覆盖层用于坐标、星表叠加、天球视角联动/孔径模板及图片导出；
取消勾选即可恢复原 WCS。原始 FITS 头、物理像素和数据定标字段不被改写，可保存 JSON 坐标解追溯来源。

## PRO spectral reference lines / PRO 光谱参考标线

Opening a spectrum in PRO selects its dedicated workspace. Below the file-path
row, two PRO command rows organize frequent/file/mode controls and grouped
display/reference/measurement/processing/fitting/comparison tasks. Mini keeps
one compact command row. **Columns**
contains Fit, uncertainty display and X/Flux/Error selection; **Files** contains
adding paths and copying session JSON. **Info** keeps the reference table,
vacuum/air conventions and help. The side panel begins with unit/redshift or the
selected tool's controls, without duplicate titles or selectors. Image tools and
comparison/link controls are hidden. Returning to an image
restores image tools and the default star finder, preserving shared workspace data.
Toggle individual
lines, doublets or groups, choose the wavelength unit when unspecified, and set
the observed air/vacuum convention. The SDSS reference table uses vacuum Angstrom
wavelengths. Orange reference lines move with user redshift; green candidate ticks
stay at the detected observed wavelength. Their visibility controls are separate.
The redshift slider has a fine numeric input; marker width does not set matching
tolerance. **Find candidate emission / absorption** subtracts a running-median
continuum and calls the selected environment's Specutils derivative or threshold
finder. Threshold finding requires the selected uncertainty column. Tasks support
cancel; leaving PRO releases their state. Basic viewing never imports Specutils.

在 PRO 打开光谱时使用专用工作区。路径行下方有双行工具栏：上行放常用操作、文件与模式，
下行按显示、标线、测量、处理、拟合和比较分组。极简保留一条紧凑横栏。**选列**包含自适应、误差显示和 X/Flux/Error
选择；**文件**菜单提供添加路径与复制会话 JSON；**说明**保留参考表、真空/空气约定和帮助。
侧栏直接显示单位/红移或当前工具控件，去掉重复标题和选择器。隐藏图像专用工具及比较/联动控件，
回到图像恢复原工具和默认找星，并保留公共工作区数据。可切换单线、双线和组线，未知波长单位
需先手动选择，并确认观测横轴为空气或真空波长。SDSS 参考线表使用真空埃；橙色
参考线随用户红移移动，绿色候选短标线固定在检测到的观测波长，两者分别控制显示。
红移滑块配有精细输入；标线粗细与匹配容差独立。**寻找候选发射 / 吸收特征**在扣除
滑动中值连续谱后调用所选环境中的 Specutils，阈值法要求已选择有效误差列。任务支持
取消；退出 PRO 释放其状态。基础查看不导入 Specutils。候选不是化学鉴定或盲红移解。

See [spectrum details and limits](SPECTRA.md) for wavelength conventions,
candidate interpretation and opt-in dependency installation.

The source candidate also provides [PRO spectral analysis](SPECTRAL_ANALYSIS.md):
source-coordinate regions/masks, linear/low-order continuum, EW/pEW and basic
feature measurements, rest/velocity display axes, cached native-grid comparisons,
bounded Gaussian/Voigt models, segmented smoothing and a relative blackbody
reference. Scientific results use full unsmoothed source samples. Display previews
are bounded; measurement uncertainties require valid source sigma and state their
independent-pixel assumptions.

源码候选版另提供 [PRO 光谱分析](SPECTRAL_ANALYSIS.md)：源坐标选区/排除区、低阶连续谱、
EW/pEW 等特征测量、静止/速度显示轴、缓存原网格对照谱、有界 Gaussian/Voigt、分段平滑
和相对黑体参考。科学计算使用完整未平滑源采样；预览有点数上限，统计误差需要有效源
sigma，并明确独立像素假设。

Image and spectrum comparisons share a [layer-list workflow](IMAGE_LAYERS.md),
with visibility, order and opacity controls. The optional field-star button
matches relative image brightness to the active original using the selected
external environment; insufficient matches or unsupported conditions do not
silently change the scale.

图像与光谱比较共用[图层列表](IMAGE_LAYERS.md)，提供显隐、层序和透明度。图像的场星
按钮调用外部环境，将相对亮度匹配到当前原图；匹配不足或条件不满足时不悄悄改比例。

## Measurements and limits / 测量与边界

- Apertures measure original physical pixels with subpixel coverage, clipped
  background and shared-pixel covariance. Without an error map or an explicit
  electron gain/read-noise model, total error/SNR remain unspecified. Weighted
  sums are not automatically calibrated magnitudes or integrated surface brightness.
  孔径对原始物理像素加权，处理背景剔除及共享像素协方差；无误差图或明确增益/读噪模型时，
  总误差和 SNR 留空。加权总和不会自动转换为定标星等或面亮度积分。
- Simple star finding uses Gaussian local maxima and positive-weight centroids
  with a global background. Crowded blends, strong background gradients and masked
  cores can bias detections; inspect flags and manually correct candidates.
  简单找星使用全局背景、Gaussian 局部峰和正权重质心；拥挤混叠、强背景梯度及被掩膜的核心
  可能影响结果，应检查质量标记并手动修正候选。宽度估计不是 PSF 拟合。
- Sky templates transform sampled boundaries and retain holes; this is a bounded
  polygon approximation. Preview before applying; target measurements are recomputed.
  天球模板变换采样边界并保留孔洞，属于有限多边形近似；确认预览后应用，目标测量重新计算。
- Trace profiles are interpolated boxcar samples. Entire-band aperture sums are
  a separate measurement. No wavelength/response calibration or optimal extraction
  is inferred. 迹线剖面为插值 boxcar 采样，与整个带状孔径的总和分别记录，不自动完成波长/响应定标或最优提取。
- Reprojection uses bilinear visualization resampling and is not flux conserving.
  RGB is display composition; photometry uses original FITS pixels. 重投影用于双线性可视化，
  不保证通量守恒；RGB 是显示合成，测光使用原始 FITS。
- CDS uploads only row identifiers/J2000 coordinates and explicit `upload_columns`;
  local XMatch performs no upload. SAMP remote access needs the optional local
  bridge and a real Hub. CDS 默认仅上传行标识、J2000 坐标及明确选择的附加列；本地匹配不上传。
  远程 SAMP 需可选本机桥接和真实 Hub，协议测试不等于所有 TOPCAT/Aladin 版本已验证。
- MathText is a subset rendered by the user's Matplotlib with `usetex=False`.
  No TeX engine or Java runtime is bundled. 数学标注由用户环境的 Matplotlib MathText 子集渲染，
  强制 `usetex=False`，不内嵌 TeX 或 Java。
- The independent offline HTML has a 64 MiB input/outer-gzip limit and disables
  Python science tools. Shared FITS decoder limits, textures and buffers can make
  memory usage exceed 64 MiB. A 128 MiB multi-view LRU is a cache budget, not a
  total process/GPU memory ceiling. 离线 HTML 的 64 MiB 是输入/外层 gzip 限制，并非总内存上限；
  Python 科学工具不可用。128 MiB 多图 LRU 也不代表进程或显存总上限。
