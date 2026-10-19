# Actual 0.1.6 screenshots / 实际界面截图

The thirteen module images are untouched PNG captures of the **installed Linux
Full10 wheel**, at 1522 × 879. The first three actually run in Minimal mode within
Full. They show real loaded M67/SDSS data and their controls; no external scientific
jobs were run just to make these illustrations. Basic aperture sums visible in the
UI are actual Rust calculations, with unknown units/missing uncertainties retained.
No scientific plot, result, label or data was painted into a screenshot.

十三张模块图来自实际安装后的 Linux Full10，PNG 1522×879 原字节复制，没有重编码
或编辑；前三张确实为 Full 中的极简模式。真实 M67/SDSS 已加载，但展示配置不等于
已经完成 Gaia、星表查询、自动谱线鉴定或拟合。孔径页的快速净和是实际 Rust 计算；
未知单位及未提供误差/SNR 仍明确显示。

The aperture popup was moved with a real drag so the circle/sky annulus remain
visible. Coordinates waited for the real asynchronous original WCS response:
FK5 J2000 RA≈132.8339524724°, Dec≈11.8118222406°. No Gaia solution was injected.
The comparison page has one actual spectrum loaded and explicitly shows the
second-spectrum prerequisite; no two-target or time-series result is claimed.
The model-fit image shows its primary action, not a newly completed fit.
The archive configuration uses the actual default 600px layout. A separate actual
resize/switch/restore/reset sequence verified persistence and reset to 600px.

`archive-download-result.png` is a separately labelled **source-9 actual workflow**:
Nearby observations → CDS/P/DSS2/red → 512×512 FITS cutout → persistent save beside
the original → automatic second tab. The saved file has 262144 finite samples,
529920 bytes and SHA-256
`c690296c95f1dc5c5300480a3bbbb8bb9f67130fed08200acac05d6f4fd3b5cc`.
Reselecting the original retains its aperture/tab and original SHA-256.
This older run explicitly set the archive height to 600px; it does not establish
the final default, which was separately checked with Full10. The product is a
resampled HiPS cutout, not an independent original exposure or photometric standard.

下载结果图另标为第九检查点的真实操作，不能拿它证明第十检查点默认高度；真正的
默认值、拖动、记忆和重置已分别实测。DSS 图为重采样切图，不是独立原始曝光或测光标准。

## Data and attribution / 数据与署名

- **M67:** STScI Digitized Sky Survey, NGS-POSS POSS-I Red photographic plate,
  distributed by Astropy/Photutils as `M6707HH.fits`.
  [Photutils source description](https://photutils.readthedocs.io/en/stable/api/photutils.datasets.load_star_image.html),
  [original FITS](https://www.astropy.org/astropy-data/photometry/M6707HH.fits).
  SHA-256 `b213e11a44a23d09ff48a42165479d21ef63620531c917020b8ff6afb0997418`.
  This is a photographic scan, not a linear CCD exposure. / 底片扫描，并非线性 CCD 计数。
- **SDSS:** plate0751, MJD52251, fiber0160,
  [DR14 public SAS FITS](https://data.sdss.org/sas/dr14/sdss/spectro/redux/26/spectra/0751/spec-0751-52251-0160.fits),
  [official optical spectrum access](https://www.sdss4.org/dr17/spectro/spectro_access/).
  SHA-256 `9bb725b4cf74cc5b547bdffa11d8cf3bcb4949cdb21b9173163fcb500eb85fb6`.
  COADD has 3841 samples, 3813 valid. Missing source units are not guessed;
  choose a wavelength interpretation before candidate detection. / 原单位未知时先显式选择解释。
- **DSS2/red cutout:** STScI DSS/POSS imagery distributed through the CDS/Aladin
  HiPS service. [CDS HiPS](https://aladin.cds.unistra.fr/hips/).

Only screenshots are included, not scientific FITS data. Per-image hashes, mode,
context and acceptance scope are in [captures.json](captures.json). The historical
JWST hero's separate attribution remains in [the preview notes](../README.md).
