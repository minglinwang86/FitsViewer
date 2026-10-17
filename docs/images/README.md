# SMACS 0723 interface preview / 主页预览来源

`jwst-smacs0723-interface.png` is an actual screenshot of FitsViewer displaying the gravitational-lensing cluster core in SMACS 0723, the field of [Webb's First Deep Field](https://science.nasa.gov/asset/webb/webbs-first-deep-field-nircam-image/).

- Observations: NASA, ESA, CSA, STScI; JWST/NIRCam, F277W.
- FITS mosaic and reduction: Gabriel Brammer (2022), *Images and catalogs of HST and JWST images in the SMACS-0723 field*, version 0.5, [Zenodo DOI 10.5281/zenodo.6874301](https://doi.org/10.5281/zenodo.6874301).
- Data license: [Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/). The FITS-derived image content retains that license; the viewer interface is covered by this project's software license.
- Source file: `smacs0723-f277w_drz_sci.fits.gz`; published MD5 `c0cf2a00fd7f7dd55e967303f7ef6b1a`, verified after download.
- Preview: a 2100 × 2100 pixel, approximately 1.4 × 1.4 arcminute cutout centered on mosaic pixel (2654, 5350), using zero-based coordinates. The WCS gives approximately RA 110.8269603°, Dec −73.4529710°.
- Display: grayscale, Asinh stretch, manual limits −0.00563975 to 0.125, bias 0.5 and contrast 1. The screenshot was captured at 1280 × 1000 pixels from the running application. It shows a single science band; the familiar outreach image combines several filters.

The source is a community first-pass reduction. Its mosaic header contains `TELESCOP=HST` and `INSTRUME=WFC3`, inconsistent with the published JWST F277W file identity; attribution here follows the dataset record and filter rather than those header values. The cutout retains the source pixel values and WCS; no image was painted into the interface.

The large source FITS mosaic is not included in the repository. Download the named file from the DOI record to reproduce the preview. The bundled synthetic FITS examples remain available for lightweight testing.

中文：主页展示程序读取真实 JWST/NIRCam F277W 科学数据的运行截图，截取 SMACS 0723 星系团核心的约 1.4 角分视场。画面为单波段灰度显示，使用 Asinh 拉伸；数据提供者、许可、校验值和裁剪参数如上。数据头中的 HST/WFC3 字段与 F277W 文件归属不一致，本文按数据提供者的记录和滤光片标注观测来源。仓库仅包含截图与来源说明，不附带大型原始 FITS 数据。
