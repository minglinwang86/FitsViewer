# PRO spectral analysis / PRO 光谱分析

This documents the 0.1.5rc1 Full spectral tools. Use the dedicated spectrum PRO
workspace; Mini allocates no analysis state. Scientific packages come from the
selected Python/Conda environment and are never embedded in the viewer.

本文描述0.1.5rc1 全功能光谱工具。工具位于光谱专用 PRO 工作区，极简模式不创建分析状态。
科学计算调用用户所选 Python/Conda 环境，不内嵌解释器、科学包、模板库或 TeX。

## Workflow / 操作流程

Choose an explicit wavelength unit when the source does not declare one. Define a
line region, continuum windows on either side and optional excluded intervals.
Use the task toolbar to select measurement, processing, fitting or comparison.
All saved intervals use the original source x values; changing the displayed axis
does not change them or the file. Results can be copied as structured JSON.

源文件未声明波长单位时，先显式选择埃、nm 或 um。设置线区、两侧连续谱窗与排除区，
再从顶部任务栏选择测量、处理、拟合或比较。区间始终以源横轴数值保存；改变显示坐标
不会改写选区、采样点或文件。结果可复制为包含参数与状态的 JSON。

Spectrum PRO uses two command rows below the separate path row: frequent actions,
files and mode controls, followed by grouped scientific tasks. Details stay in the
resizable work area. Hiding the work area retains PRO state; switching to Mini
releases it. The bottom readout remains two fixed rows.

光谱 PRO 在独立路径行下使用双行工具栏：常用操作、文件与模式在上，按组组织的科学
任务在下。详细参数放在可调宽度的工作区，收起保留状态，切到极简释放 PRO 状态。
底部读数固定两行。

## Tools and scientific conventions / 工具与计算约定

| Tool / 工具 | Behavior and limits / 行为与限制 |
|---|---|
| Regions, continuum windows, masks / 选区、连续谱窗、排除区 | Keep the original sample correspondence. Excluding samples does not delete or overwrite them. / 保留源采样对应关系，排除不等于删除或覆盖文件。 |
| Continuum / 连续谱 | Linear or degree 0–5 polynomial least squares in scaled x, optionally weighted by positive source sigma. Exclude the line region; no hidden sigma clipping. Show baseline, normalized or subtracted preview. / 线性或 0–5 阶多项式，缩放横轴以改善数值条件；有完整有效误差时加权。拟合排除线区，不暗中裁剪离群点，可对照基线、归一谱与扣除谱。 |
| Measurements / 特征测量 | EW/pEW is integral(1-flux/continuum) d(source x): absorption positive, emission negative. Also report net continuum-subtracted flux, original-sample minimum, centroid and FWHM status. / EW/pEW 对吸收为正、发射为负；同时给净积分流量、源采样最低点、质心与半高全宽状态。 |
| Observed/rest/velocity / 观测、静止与速度轴 | Display coordinate transforms only. Rest wavelength divides by 1+z; relativistic velocity uses a user reference. Flux density is not transformed. / 仅变换显示横轴；静止波长除以 1+z，速度使用指定参考线与相对论约定，不进行流量密度物理换算。 |
| Native-grid overlays / 原网格叠图 | Reuse cached source arrays, up to eight enabled comparisons. Each has explicit wavelength unit, z, display scale/offset and epoch/phase label. No resampling, flux-unit conversion, coadd or automatic time interpretation. / 复用缓存，最多八条启用的对照谱；分别确认单位、红移、显示比例、错位和历元标签，不重采样、不换流量单位、不合并不同历元，也不推断标签的时间零点。 |
| Bounded profiles / 有界线形拟合 | One to four Gaussian or peak-normalized Voigt components. Continuum is fitted first and held fixed; models, components and residuals are previews. / 1–4 个 Gaussian 或峰高归一的 Voigt 分量；先拟合并固定连续谱，显示总模型、分量和残差。 |
| Smoothing / 平滑 | Boxcar, Gaussian or Savitzky–Golay on separate valid segments. Window/sigma are sample counts; Gaussian/SG require a grid uniform within 1%. Formal measurements still read the unsmoothed source. / 各有效段分别处理，窗口与 sigma 按采样点计；Gaussian/SG 要求网格在 1% 内均匀。正式测量始终读取未平滑源谱。 |
| Blackbody reference / 黑体参考曲线 | A relative Planck B_lambda shape with adjustable temperature and display amplitude. No automatic physical temperature, extinction or radius inference. / 可调温度与显示幅度的相对 B_lambda 形状，不自动推断真实温度、消光或半径。 |

The continuum is a user-defined local or pseudo-continuum, especially for blended
supernova features. Its normalized preview is undefined when the baseline is
nonpositive. Known frequency/pixel/index axes cannot be re-labeled as wavelength.
Unit/air choices are interpretations of the source; no barycentric correction or
implicit air/vacuum rewrite is performed.

连续谱是用户选窗定义的局部基线；对超新星混叠特征应称为伪连续谱。基线非正处不能
归一化。已知频率、像素或序号横轴不能假装成波长。单位与空气/真空选项解释源坐标，
不隐式实施重心修正或改写波长。

Measurements use the complete native sample span inside the requested region,
without extrapolating its fractional endpoints. A masked/nonfinite sample or large
gap makes the integral incomplete; the tool returns that status instead of
bridging it. A centroid with nearly cancelling positive/negative residuals or a
multi-peaked/truncated FWHM may be undefined. Widths include instrumental LSF.

积分使用请求区内完整的源采样跨度，不向区间的非采样端点外推。掩膜、非有限点或大
缺口导致积分不完整时直接返回状态，不把线翼拼起来。正负残差互相抵消的质心、多峰或
截断线翼的半高宽可能无定义。宽度包含仪器 LSF。

When all needed source sigma values are positive and finite, optional seeded
Monte Carlo perturbs the original pixels and refits the continuum on each draw.
The seed, successful counts and per-metric uncertainty status are recorded.
Missing errors produce values without formal statistical uncertainties. Pixel
covariance, window choice, calibration, z, LSF and continuum systematics remain
unassessed. Smoothed errors are correlated and are not used as independent input.

所需区域的源 sigma 全部有效且为正时，可用固定随机种子的 Monte Carlo 扰动原采样，
每次重新拟合连续谱，保存种子、成功次数与各指标误差状态。无误差列时仅给数值，不
伪造统计误差。像素协方差、选窗、标定、红移、LSF 等系统误差仍未评估；平滑后的相邻
误差相关，不能当作独立输入。

Profile covariance, when available, is local and conditional on the fixed
continuum and independent source errors. Boundary hits and rank deficiencies are
reported. Gaussian sigma and Voigt Gaussian/Lorentz FWHM are distinct parameters;
neither individual Voigt width is the full Voigt FWHM. A plausible component fit
is not evidence of a physical species, ejecta shell or unique decomposition.

线形协方差仅为固定连续谱、独立源误差假设下的局部估计；触边与秩亏有明确状态。
Gaussian 的 sigma、Voigt 的 Gaussian/Lorentz 半高宽是不同参数，后两者任意一个都
不是 Voigt 总半高宽。拟合外观合理不能证明元素、物质壳层或唯一物理分解。

## Reproducible external call / 可复现外部调用

Continuum, measurements, smoothing and bounded profiles require only external
NumPy and SciPy; candidate line finding additionally uses Astropy and Specutils.
The current optional group can be installed into the selected environment with
`python -m pip install -r requirements-spectra.txt` from the source root. This is
an explicit environment operation; opening a file never installs packages.

连续谱、特征测量、平滑与有界线形拟合只需外部 NumPy、SciPy；候选找线另需 Astropy、
Specutils。可在源码根目录，对明确选定的环境手动执行
`python -m pip install -r requirements-spectra.txt`；打开文件不会自动安装依赖。

Save a parameter file, adjusting the path, units and windows for the actual data:

```json
{
  "path": "data/spectrum.fits",
  "hdu": 1,
  "unit": "Angstrom",
  "action": "measure",
  "region": [5060, 5110],
  "continuum_windows": [[5010, 5045], [5130, 5160]],
  "masks": [],
  "continuum_method": "linear",
  "redshift": 0.0,
  "mc_samples": 128,
  "seed": 20261017
}
```

```bash
fitsviewer science spectral_analysis --params parameters.json --python /path/to/env/bin/python
```

`action` is `continuum`, `measure`, `smooth` or `fit`. Measurement and fitting
require an explicit region. Results include source context, parameters, package
versions, status, uncertainty assumptions and bounded preview arrays. Source,
selection and configuration revisions protect the viewer from late incompatible
results. Numerical calculations always use full source samples; display arrays
are capped at 8192 samples per series with gaps retained.

`action` 可选 `continuum`、`measure`、`smooth`、`fit`；测量与拟合必须指定线区。
结果记录源上下文、参数、包版本、状态和误差假设。源、选列与配置修订号阻止过期结果
落到新状态。数值计算用全采样；每条预览最多 8192 点并保留断线。

Method references: [SciPy least_squares](https://docs.scipy.org/doc/scipy/reference/generated/scipy.optimize.least_squares.html),
[SciPy Voigt profile](https://docs.scipy.org/doc/scipy/reference/generated/scipy.special.voigt_profile.html),
[SciPy Savitzky–Golay](https://docs.scipy.org/doc/scipy/reference/generated/scipy.signal.savgol_filter.html),
[Astropy BlackBody](https://docs.astropy.org/en/stable/modeling/physical_models.html#blackbody),
[Astropy Doppler equivalencies](https://docs.astropy.org/en/stable/units/equivalencies.html#spectral-doppler-equivalencies).
