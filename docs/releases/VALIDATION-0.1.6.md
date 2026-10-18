# FitsViewer 0.1.6 validation / 验收记录

This record separates source checks, real numerical work, GUI actions and packaged
platform acceptance. **Release assembly is still in progress in this working copy.**
The final artifact hashes and platform outcomes must be filled before publication.
Earlier checkpoints are evidence for their own source; a changed component needs a
corresponding new check or an explicit unchanged-source comparison.

本文区分源码、真实数值、界面操作和平台发行包验收。当前工作副本尚在汇总最终产物；
只有填入最终源码／资源哈希、平台结果后才能发布，不能把阶段证据当作最终验收。

## Implemented boundaries / 已检查的边界

- One shared Rust core/viewer and module registry. Full opens Minimal without
  creating PRO state. Scientific packages and Python remain external.
- Application icons belong to the program, launchers and viewer tabs/windows;
  associated scientific files retain generic file icons.
- Display stretch, brightness, rotation, layers and previews do not rewrite source
  arrays. Apertures, picking, overlays and WCS use original detector coordinates.
- Complete decoded packets up to `20_000_000` bytes transfer once. Larger supported
  ordinary FITS images use seeked preview/tile/pixel access; compressed preparation
  and the complete-array science path retain their separate explicit limits.
- No Chromium/browser runtime, Python runtime, science data, catalogue index or TeX
  is embedded. Chromium/Playwright are validation tools, not release dependencies.

## Source and numerical evidence / 源码与数值

Runtime source checkpoint 9 was frozen as 875 files; the private source ZIP's
SHA-256 is `f856c7dbcdbe1fa59c89c6d3b1dc4728989bd0a3c37b9d05145986bc68395505`.
Post-freeze changes are documentation, illustrations, the macOS resource lock and
an explicitly recorded QA fixture correction, not runtime code changes.

| Matrix | Actual result |
| --- | --- |
| Linux/TNT Full host | 36/36 tests; all-features and no-default checks pass. |
| Windows Full host | 35/35 tests; desktop-only check passes. |
| Headless PRO host | Linux 35/35, Windows 34/34; matching-source evidence includes actual copy cancellation. |
| Viewer PRO / base | 91/91 and 45/45 on Linux/TNT and Windows. The base test-name sets were independently compared and match. |
| WASM | PRO and no-default configurations pass checks. |
| Shared core | 38 previous tests are explicitly inherited: all 16 relevant core source files match by SHA-256. They are not claimed as newly executed. |

Linux 与 Windows 的测试数差异包括 Linux 专用不可写目录回退测试。取消/失败后再
执行重投影的保存链回归包含在 91 项 Viewer 测试内。未改核心以完整文件哈希关联历史
证据，避免为累计次数重复运行。
保留首次失败：测试索引类型、旧工具名称断言、Windows 能力路径表示，以及真实 HTTP
查询提取／流式 POST 回归。测试夹具修正不放宽生产授权或科学校验条件。

Real public science data included an M67 field, HST/NICMOS SCI+ERR+DQ and an SDSS
COADD spectrum. An independent 19-check campaign contained 18 actual numerical or
workflow checks and one source/oracle velocity review. Highlights:

| Data/workflow | Result and limits |
| --- | --- |
| SDSS FITS table | Log wavelength, source flux, mask and inverse-variance-to-sigma agree with Astropy. |
| Continuum/profile fit | Continuum agrees with an independent weighted least-squares oracle; Gaussian parameters/covariance/residuals agree with SciPy. The real line's reduced χ² is about 6.71, so numerical convergence does not establish an adequate physical model. |
| Spectral candidates | Specutils emission/absorption candidates are provisional; candidate labels do not establish a species or supernova classification. |
| Air/vacuum/rest correction | 25 targeted checks, including a real SDSS vacuum-spectrum regression, verify air→vacuum→rest positions, endpoint FWHM and transformed-grid EW. Source samples and observed measurements remain unchanged. |
| M67/HST aperture | Numerical aperture/background results agree with Photutils at the declared subpixel sampling. Missing errors remain null; they do not produce a formal SNR. |
| HST quality flags | Nonzero DQ values occur in the test aperture/background. Agreement of SCI/ERR arithmetic alone is not valid final scientific photometry; masking and instrument-specific calibration remain necessary. |
| Lossless `.fz` oracle | A derived RICE re-encoding has the same decoded physical pixels as the public original. It is labelled as a derivative, not another independent observation. |

### Real Gaia regression / 真实 Gaia 日期回归

The public M67 header uses the old FITS `DATE-OBS='29/11/51'`. Five date-contract
tests verify the fixed 1900–1999 century, leap dates, invalid inputs, explicit
epochs and proper-motion agreement with Astropy. The existing 28 science-service
tests also passed after the correction.

A normal Windows-network query returned 1000 Gaia DR3 reference rows. With proper
motions propagated to **1951-11-29**, the real image solve was accepted: 98 pairs,
73 fit stars, 24 independent validation stars; fit RMS ≈0.782″ and validation RMS
≈0.925″ under the default 3″ limit. The source file's SHA-256 stayed unchanged.
This is a particular functional test, not a universal accuracy specification.

旧版直接调用通用日期解析器而失败；修复按 FITS 规范读世纪，不猜 2051 年，也不忽略
自行传播。无效日期给简短错误；独立验证不合格仍拒绝，不把门槛当测量精度保证。

### Current-field products / 当前视场产品

Thirteen independent geometry/download boundary tests pass, including spherical
solid-angle and quadrature oracles, 40%/150% endpoints, partial fallback, RA wrap,
polar and simple concave fields, malformed responses and cancellation cleanup.
The existing 28 science-service and 9 module-selection tests also pass. A corrected
manual-coordinate fixture keeps all 20 file-only table guards passing; it still
requires rejection of external STREAMs before table parsing or URL access.

Actual public M67 WCS discovery gives about 0.250214115 deg². A trusted HTTP
workflow retrieved a DSS2/red HiPS FITS cutout (128×128, 37,440 bytes), verified its
requested/returned WCS and 16,384 finite samples, saved it beside the original and
read it through viewer metadata and the Rust core. Its area ratio is about
100.0059%, with approximately full actual overlap. After artifact release and host
exit, a new read-only host still read the saved file. The original SHA-256 did not
change; no `.part` or owned process remained.

Eight actual SDSS SIA candidates had bounded official FITS-header verification,
reading 2,162,688 wire bytes in total. Five did not overlap; three had about 14.75%
area ratios and 12.0%–12.1% actual overlap and were explicitly partial fallback.
This was a real query → real headers → selection sequence, not a separate repeated
final combined network request or a claim of service-wide completeness.

HiPS samples are resampled cutouts, not independent exposures or validated
flux-conserving reprojections. Sampled 32-position boundaries approximate WCS
footprints. Unknown/complex geometry remains diagnostic; unsupported dimensions,
all-invalid or inconsistent returned cutouts are rejected.

## Actual GUI and resource evidence / 实际界面及资源

Source-7 actual Linux browser actions covered real file-list double-click,
source-candidate detection, photometry, selected-aperture Delete and deletion
after a 37° display rotation. The remaining aperture retained its original pixel
coordinates. A real SDSS fit was triggered from its primary button and completed.
Per-page captures cover English/Chinese, narrow/wide interfaces, image/spectrum
and expanded PRO tools; final capture review and stale-heartbeat diagnosis remain
part of final assembly.

For a 4800×4800 image with a 92.16 MB original array, the tested browser transferred
1,530,000 numeric preview bytes initially. Tone changes issued no new image
requests. Zoom/pan/rotation used tiles with no full-array request: 34 image
requests in the run, 6,535,923 completed response bytes and a maximum estimated
tile cache of 26,042,944 bytes under its 40 MiB budget. Closing with four pending
tiles cancelled all four; no later image request or retained remote-image state
was observed. These values describe that run, not arbitrary performance or total
RSS/GPU-memory guarantees.

A fresh isolated venv installed Requests through the actual View-role dependency
job while another View released PRO/document ownership. Installation, pip check,
preservation of existing packages, log delivery and venv interpreter identity
passed. After all owned views released, the worker stopped and no owned pip/worker
process remained. The original selected environment was restored.

## Capability and platform evidence / 能力与平台

An isolated real broker passed the existing protocol checks plus explicit-file,
owner/role/generation, ticket, cancellation, source-change, download-scope and queue
admission checks. The 165 assertions include 128 bounded-capacity loop assertions;
they are not 165 separate features. Twenty self-contained table tests reject
external VOTable STREAMs/entities and compression-budget bypasses. Windows' actual
protocol test uses the broker's canonical path spelling, with other files still
rejected. Owned broker/worker descendants were reaped after the runs.

Windows source-9 headless packet/codec acceptance (37 cases), real HTTP
metadata/full/tile/pixel values against Astropy, invalid-parameter rejection,
canonical fver protocol, 25 spectral checks and owned-process cleanup passed.
Native GPU startup and real FITS/derived `.fz`/HST/SDSS loading also have source-7
actual evidence, which is not promoted to new source-9 GUI evidence. The user preview
was installed independently of the previous version. The final Setup is checked
without silently overwriting an existing user's registration. A complete positive
Setup install/uninstall on a clean machine is a separate acceptance item.

Final Linux portable ABI/wheel, Windows artifacts, macOS arm64/x86_64 CI results,
the final source/resource hashes and the new archive-download UI are **pending in
this working record**. Do not publish with this paragraph still present.

macOS acceptance must come from real macOS runners. An app startup probe is not
full interactive GUI testing; an ad-hoc signature is not Apple notarization.
JupyterLab 5, Linux/Windows ARM, Linux musl, Safari, actual X2Go forwarding,
production PSF photometry and paper-ready scientific calibration are outside
this release's acceptance scope.
