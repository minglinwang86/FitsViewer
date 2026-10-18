# Basic spectrum viewing / 基础光谱查看

This describes the 0.1.5rc1 Full prerelease. It is a viewer: source samples
are retained and files are never rewritten.

本文描述0.1.5rc1 全功能预发布版。查看时保留源采样点，不回写文件。

| Input / 输入 | Interpretation / 解释 |
|---|---|
| 1D FITS Primary/Image HDU | Flux uses BSCALE/BZERO/BLANK; linear CRVAL1/CRPIX1/CDELT1 or CD1_1 coordinates, plus legacy DC-FLAG=1 log10 dispersion. Unsupported spectral WCS is labeled as sample indices. / 保留流量缩放，支持线性或传统 log10 坐标；不支持的波长解显示采样序号。 |
| FITS binary table | Recognized WAVE/WAVELENGTH/LAMBDA/LOGLAM and FLUX columns; scalar rows or one fixed-vector row. Apply TSCAL/TZERO/TNULL once. / 自动识别坐标和流量列，支持标量行或单行定长向量。 |
| UTF-8 `.txt`, `.dat`, `.csv`, `.ascii` | At least two numeric columns; whitespace, comma or semicolon separators. Optional column names; comments start with `#` or `!`; D exponents, NaN and empty CSV values are accepted. Units are unknown unless supplied by a supported FITS header. / 至少两列数值，可带列名与注释；未知单位明确标注。 |

FITS files open by double-click in JupyterLab when FitsViewer is the selected
default. For text files use **Open With → FitsViewer**; their existing default
editor remains unchanged. Desktop **Browse** and the command-line accept all of
the formats above. Externally gzip-compressed versions are read by the same core.

JupyterLab 中默认关联的 FITS 可双击打开。普通文本使用 **Open With → FitsViewer**，
其默认编辑器保持原设置。桌面文件选择器和命令行也接受这些格式，外部 gzip 压缩由
同一读取内核处理。

```bash
fitsviewer --native spectrum.fits
fitsviewer spectrum.dat
```

```python
from fitsviewer import show
show("中文数据/光谱.fits", height=600)
```

Scroll to zoom around the pointer, drag with the left mouse button to pan, and
press **F** or **Fit spectrum** to fit the data. The cursor reports the nearest
original sample. If table/text columns are selectable, choose **X**, **Flux** and
**Error**; choose **None** to hide an error column. The **Error bars** checkbox
controls display without changing the data.

滚轮以鼠标位置为中心缩放，左键拖动平移，**F** 或 **Fit spectrum** 恢复自适应。
游标读出最近的原始采样点。表格或文本可选择 **X**、**Flux**、**Error** 列，选择
**None** 禁用误差列；**Error bars** 复选框只控制误差显示。

Recognized ERR/ERROR/SIGMA columns are standard deviations. Variance is converted
with a square root and positive IVAR with an inverse square root. Invalid errors
are hidden. Recognized MASK/DQ/QUALITY/AND_MASK columns flag nonzero samples as
invalid. Source units are shown without unit conversion. NaNs break the line;
display reduction retains each bin's extrema and original order. A reduced plot
is not a resampled scientific spectrum.

已知误差列按标准差、方差或逆方差转换；无效误差不绘制。已知质量掩膜中非零值
对应的流量视为无效。单位沿用源文件，不自动换算。无效值处断线，大量点绘制时
保留各组极值及顺序；显示降采样不生成新的科研光谱。

## PRO reference lines and candidate features / PRO 标线与候选特征

In PRO, a spectrum opens **Spectral reference lines** by default. Its dedicated
workspace uses two command rows below the separate file-path row in PRO: frequent
actions/files/mode above, grouped display, references, measurement, processing,
fitting and comparison below. Mini keeps a compact single command row.
**Columns** holds Fit, uncertainty display and
X/Flux/Error selection (including None); **Files** holds Add files and Copy session
JSON, and **Info** holds the reference table, wavelength conventions and help.
The side panel starts directly at unit/redshift or the selected tool controls.
Image photometry, coordinate calibration, star
finding, catalogue overlays, reprojection/RGB and image comparisons are hidden.
Switching back to an image restores its tools and default star finder, retaining
shared workspace data. File tabs, adding paths and copying session JSON remain
available in both data contexts. Switch individual reference lines,
doublets or groups on/off; use the redshift slider and fine input to set
`lambda_observed = lambda_rest * (1 + z)` in vacuum. Only finite `-1 < z <= 20`
is accepted. These are reference positions; broad stellar/supernova absorption
can be displaced by ejecta velocity, blending or continuum placement.

PRO 下打开光谱时默认选择谱线工具；独立路径行下方使用双行工具栏：上行放常用操作、
文件与模式，下行按显示、标线、测量、处理、拟合和比较分组。极简保留紧凑单行。
**选列**包含自适应、
误差显示和 X/Flux/Error（包括 None）；**文件**提供添加路径与复制会话 JSON；**说明**
保留参考表、波长约定和帮助。侧栏直接从单位/红移或当前工具开始，隐藏图像测光、
坐标校准、找星、星表叠加、重投影/RGB 和图像比较。
切回图像恢复对应工具与默认找星，同时保留公共工作区数据。两种数据上下文都保留
文件标签、添加路径和复制会话 JSON。
支持单线、双线和组线显示开关，红移滑块与精细输入按真空波长关系
`lambda_observed = lambda_rest * (1 + z)` 移动参考位置，仅接受有限的 `-1 < z <= 20`。
这些是参考位置；恒星或超新星宽吸收特征可能受速度、混叠和连续谱选择影响而偏离。

The small [SDSS SPECTRO1D reference table](https://classic.sdss.org/dr7/algorithms/linestable.php)
is in vacuum Angstrom. Some UV entries are blended reference features. Unknown
axis units require an explicit Angstrom/nm/um choice, including SDSS LOGLAM
columns whose values were already exponentiated by the reader. Sample indices,
frequency and velocity axes are rejected. If the observed axis is in air,
reference positions use the [SDSS Ciddor conversion](https://sdss.org/dr18/tutorials/conversions/)
after applying redshift, restricted to observed 2000–20000 Angstrom. The plotted
samples are never converted or rewritten.
The plot axis shows an explicitly overridden unit as a manual interpretation;
source metadata remains unchanged, and leaving PRO restores its original label.

小型 [SDSS 参考线表](https://classic.sdss.org/dr7/algorithms/linestable.php)使用真空埃；
部分紫外条目是复合特征。未知横轴单位必须显式选择埃、nm 或 um；SDSS LOGLAM
列已由读取器取 10 的幂，但元数据单位可能仍为空。采样序号、频率与速度轴不能匹配。
空气波长轴采用 [SDSS Ciddor 换算](https://sdss.org/dr18/tutorials/conversions/)，先应用
红移再换算，并限制于观测波长 2000–20000 埃。原始波长和流量不会改写。手动指定
单位会在横轴注明“手动解释”；源元数据保持原样，退出 PRO 恢复原标签。

Install optional packages only into the Python/Conda environment selected for
science tools; they are not viewer runtime dependencies:

```bash
python -m pip install -r requirements-spectra.txt
```

**Find candidate emission / absorption** uses the same shared-core data and column
selection as the visible plot. Each contiguous valid segment has a running-median
continuum subtracted; Specutils then finds derivative extrema or thresholds using
the chosen noise factor. Threshold mode requires a valid selected error column.
Derivative mode uses the median source standard deviation when usable, otherwise
a MAD estimate from adjacent residual differences. Gaps stay separate. Duplicate
or nonmonotonic wavelengths are rejected; descending spectra are supported. Results
record package versions, continuum window, method and noise estimate. At most 256
candidates are kept and plotted as green short ticks. Up to 64 of the strongest
features receive `?` labels when space permits; text uses three collision-checked
lanes and is omitted when crowded. Hover near a bottom tick for full information,
or inspect the complete candidate list. Orange reference lines stay under the
user's manual control.

**寻找候选发射 / 吸收特征**使用与当前图相同的共用内核数据及选列，对每段连续有效数据
扣除滑动中值连续谱，再调用 Specutils 导数或阈值方法。阈值法要求有效的误差列；导数法
优先用有效源标准差中值，否则用相邻残差差分的 MAD 估计。掩膜间隙不拼接，拒绝重复或
非单调波长，支持递减波长。结果保存包版本、连续谱窗口、方法和噪声估计。最多保留
256 个候选，并绘制绿色短标线。最多给 64 个强特征显示问号标签，文字分三层检查冲突，
拥挤时省略文字；悬停在底部短标线附近或查看完整列表可读出信息。橙色参考线仍由用户手动控制。

Matching uses an independent tolerance in observed Angstrom and the current user
redshift. Changing redshift updates possible line names locally; changing the
unit or air convention clears incompatible candidates. Candidate extrema and
wavelength proximity do not establish species, significance, velocity or redshift.
Running-median subtraction can suppress broad lines; contrast/noise values are
screening quantities, not formal detection significance. The small reference list
is incomplete. Use specialist continuum fitting, uncertainties and multi-line
evidence for scientific interpretation. Detection runs as a cancellable owned
background job in PRO; Minimal allocates no line state and imports no external
spectral package.

For a reproducible stateless check, save a JSON parameter file containing `path`,
`hdu`, `unit`, `method`, `sigma` and `continuum_window`, then run
`fitsviewer science spectral_candidates --params parameters.json --python /path/to/env/bin/python`.
The viewer command `spectral.lines` changes only PRO overlay settings; `pro.run`
with operation `spectral_candidates` starts detection. Candidate results preserve
their source/column/unit/air context so incompatible late replies are ignored.

匹配使用独立的观测埃容差和当前用户红移。调红移仅本地更新可能线名；切换单位或
空气约定会清除不兼容候选。极值与波长接近不能确定元素、显著性、速度或红移。
滑动中值可能压低宽线，残差与噪声之比仅用于筛选，不是正式检测显著性；小型线表不完整。
科学解释仍需专门连续谱拟合、误差与多线证据。检测是 PRO 中可取消且按所有者管理的
后台任务；极简不分配标线状态，也不导入外部光谱包。

Additional source-candidate PRO tools are documented in
[spectral analysis](SPECTRAL_ANALYSIS.md): regions/masks, local continuum and
normalization, EW/pEW and line metrics, rest/velocity axes, native-grid overlays,
bounded Gaussian/Voigt profiles, smoothing and an adjustable blackbody reference.
They use the original data, selected external environment and explicit unit/error
assumptions; none is a definitive species identification or temperature solution.

新增源码候选功能详见[光谱分析](SPECTRAL_ANALYSIS.md)：选区/排除区、连续谱与归一化、
EW/pEW 等特征测量、静止/速度轴、原网格叠图、有界 Gaussian/Voigt、平滑和可调黑体
参考曲线。使用源数据、所选外部环境及明确的单位/误差前提，不自动确认元素或真实温度。

Limits: 1,000,000 samples; 32 MiB text input; 512 MiB input/decompressed FITS.
Variable-length arrays, multiple vector rows/orders, IRAF multispec, complete
nonlinear spectral WCS, rewriting converted spectra and definitive line identification are
outside this candidate. HCOMPRESS/PLIO and compressed null-mask columns are not
supported. Errors are explicit rather than silently selecting a spectral order.

限制为 100 万个采样点、32 MiB 文本、512 MiB FITS/解压数据。暂不支持变长数组、
多行向量或多阶光谱、IRAF multispec、完整非线性光谱 WCS、改写单位换算后的光谱、
确定性谱线鉴定；HCOMPRESS/PLIO 及压缩空值掩膜仍不支持。
