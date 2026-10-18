# Screenshot provenance / 截图来源

These are actual Linux browser screenshots of the Full 0.1.5rc1 candidate's
current ribbon and workspaces. The Minimal examples show Minimal mode in Full;
they do not represent a separate Mini binary. The `.jpg` files are exactly the
original browser-provided JPEG capture bytes at 1280 × 720. No additional JPEG
encoding or image editing was performed. Scientific plots and task results were
not painted in or replaced. Only screenshots are included here, not the
scientific FITS files.

这些图片来自 Full 0.1.5rc1 候选版的真实 Linux 浏览器运行界面。极简图展示 Full 中的
极简模式，不代表独立 Mini 构建。`.jpg` 文件完整保留浏览器直接返回的原始 JPEG 字节，
尺寸为 1280 × 720，没有再次 JPEG 编码或图片编辑；没有替换科学曲线或伪造任务结果。
本目录只包含截图，不附带科学 FITS 数据。

## Data credits / 数据致谢

- **M67:** NGS-POSS POSS-I Red photographic plates, digitized by the STScI
  Digitized Sky Survey, distributed as the Astropy/Photutils sample
  `M6707HH.fits`. [Photutils description](https://photutils.readthedocs.io/en/stable/api/photutils.datasets.load_star_image.html),
  [original FITS](https://www.astropy.org/astropy-data/photometry/M6707HH.fits).
  This is a photographic scan, not a linear CCD-count exposure.
  / M67 来自 POSS-I 红光底片的数字化扫描，不是线性 CCD 计数图。
- **HST/NICMOS:** NASA/ESA Hubble sample `hst-nicmos.fits`, target NGC 4151,
  SCI HDU 1 and ERR HDU 2. [NASA FITS sample page](https://science.data.nasa.gov/fits/sample-files),
  [original FITS](https://science.data.nasa.gov/sites/default/files/2026-06/hst-nicmos.fits).
  / HST/NICMOS 公开样本，目标 NGC 4151；科学图和误差图分别为 HDU 1 与 2。
- **SDSS:** the main spectrum is plate 0751, MJD 52251, fiber 0160
  ([DR14 SAS FITS](https://data.sdss.org/sas/dr14/sdss/spectro/redux/26/spectra/0751/spec-0751-52251-0160.fits)).
  The comparison is SDSS DR12 BOSS plate 3586, MJD 55181, fiber 0198
  ([SAS FITS](https://data.sdss.org/sas/dr12/boss/spectro/redux/v5_7_0/spectra/3586/spec-3586-55181-0198.fits)).
  See [SDSS optical spectral access](https://www.sdss4.org/dr17/spectro/spectro_access/).
  These are different targets with different native grids, not two epochs of one
  object. / 两份 SDSS 光谱来自不同目标和不同原始网格，不是同一目标的两个历元。
- The `M67.fits.fz` layer is an explicitly derived, lossless Astropy GZIP_1 storage
  representation of the same M67 pixels (1,316,160 bytes). It is not another observation.
  / 压缩图层是同一 M67 物理像素的无损 GZIP_1 存储表示，不是另一幅独立观测。

The filenames visible in the UI are generic aliases. Source-byte SHA-256:
界面中使用通用文件别名，源文件字节的 SHA-256 为：

| Alias / 别名 | SHA-256 |
|---|---|
| `M67.fits` | `b213e11a44a23d09ff48a42165479d21ef63620531c917020b8ff6afb0997418` |
| `M67.fits.fz` | `f8bd039d59ec6194de363c842d82b59bc527196175738e91d23398c210da1479` |
| `HST-NICMOS.fits` | `c1c1652945cc2bff4a912d7393e15d9afc0cd3ff1de097591405ad7c9e7895d2` |
| `SDSS-spectrum.fits` | `9bb725b4cf74cc5b547bdffa11d8cf3bcb4949cdb21b9173163fcb500eb85fb6` |
| `SDSS-comparison.fits` | `6bd7afece440a89e1321751c15729a7c3d24c883969391bd91a73c7b96e23d73` |

## What the screenshots establish / 截图的实际验收范围

| Image | Actual result or configuration / 实际结果或配置 |
|---|---|
| `mini-image.jpg` | Real M67 image, Auto display, Asinh stretch and colorbar. / 真实 M67 图像、自动显示、Asinh 拉伸和色标。 |
| `mini-aperture.jpg` | A real star's circle and sky annulus. Unknown unit and unavailable error/SNR are explicit; no calibrated magnitude is inferred. / 真实星点圆孔径与背景环；未知单位、缺失误差和 SNR 明示，不推断定标星等。 |
| `mini-spectrum.jpg` | Actual SDSS COADD HDU 1, native `loglam`/`flux`/`ivar`, masks and uncertainty controls; 3841 samples, 3813 valid. / 真实列映射、掩膜及误差显示，保留源单位限制。 |
| `pro-layers.jpg` | **Completed:** 125 field stars give relative scale 1.000000 between M67 and its lossless storage representation; auxiliary opacity 0.50. Unverified-filter display comparison was explicitly allowed. This is a consistency example, not absolute calibration. / **已完成：**同一观测的两种存储表示匹配 125 颗场星，比例为 1；已显式允许未知滤镜的显示比较，不是绝对定标。 |
| `pro-photometry.jpg` | **Completed:** NICMOS SCI 1 with same-file ERR 2 interpreted as sigma, COUNTS/S retained. Circle r=8 px, sky 12–18 px; net≈33383.73, error≈13.73, SNR≈2431.82. DQ/systematics, PSF/aperture corrections and absolute calibration were not assessed. / **已完成：**显式误差源测量；未评估 DQ、系统误差、PSF/孔径修正或绝对定标。 |
| `pro-coordinates.jpg` | **Measured:** original M67 WCS center, J2000 RA=132.8339524725°, Dec=11.8118222406°. **Configuration only:** Gaia options and the 1 arcsec residual limit; no Gaia query/fit was run. / **已实测：**原 WCS 中心坐标；**仅配置：**Gaia 选项与门槛，不是已达到的坐标精度。 |
| `pro-detection.jpg` | **Completed:** 100 source candidates (limit 100), FWHM 3 px, separation 4 px, threshold 5 sigma. Trace controls are visible but were not run. / **已完成：**100 个候选；迹线检测只展示入口，没有运行。 |
| `pro-catalogues.jpg` | **Configuration only:** TAP/CDS query parameters and catalogue/XMatch/VO/SAMP entry points. No online result, uploaded match or SAMP connection is claimed. / **仅配置：**未提交在线查询、CDS 匹配或 SAMP 连接。 |
| `pro-export.jpg` | **Completed:** 1600×1200 PNG, 150 dpi, colorbar, WCS axes and compass. RGB, reprojection and MathText are visible entry points, not executed results in this capture. / **已完成：**带 WCS 与方向标记的 PNG 导出；RGB、重投影与数学标注未在本截图中执行。 |
| `pro-spectrum-lines.jpg` | Explicit Angstrom interpretation and manually set z=0.0455906, Balmer/[O III] reference overlays. Candidate finding was not run; markers do not identify a species or solve a redshift. / 显式单位和手动红移参考标线；未执行候选检测，不代表元素鉴定或自动红移解。 |
| `pro-spectrum-analysis.jpg` | **Completed:** local continuum and seeded native-sample feature measurement, region 5220–5255 Å, continuum 5180–5210/5260–5290 Å, 128 Monte Carlo draws. Observed pEW≈−9.89245±0.264 Å (emission-positive flux gives negative EW). Unknown source flux units remain unknown; errors assume independent source pixels. / **已完成：**局部连续谱和特征测量；发射 EW 为负，保留未知流量单位与独立像素误差假设。 |
| `pro-spectrum-compare.jpg` | **Displayed:** two actual SDSS sources on native grids, second source scale 1, offset +20, opacity 1, explicit Angstrom interpretation. Display offset is not physical flux conversion, resampling, coaddition or a time-series analysis. / **实际叠图：**第二个目标仅作显示错位，不做物理流量换算、重采样、合并或时序解释。 |

Optional alternatives: `mini-coordinates.jpg` shows the same shared WCS panel in
Minimal mode; `pro-spectrum-processing.jpg` shows an actually generated
7-sample segmented boxcar preview over the unsmoothed source. Scientific
measurements continue to use the original samples.

备选图：`mini-coordinates.jpg` 展示极简模式共用坐标面板；
`pro-spectrum-processing.jpg` 展示实际生成的 7 采样点分段 boxcar 平滑预览。
正式科学测量继续使用未平滑原始采样。


