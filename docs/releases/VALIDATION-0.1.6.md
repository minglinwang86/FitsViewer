# FitsViewer 0.1.6 validation / 验收记录

This record separates source checks, real numerical work, GUI actions and packaged
platform acceptance. Final Full packages use runtime checkpoint 10.
Changed components were checked again; unchanged evidence is explicitly inherited.
Earlier checkpoints are evidence for their own source; a changed component needs a
corresponding new check or an explicit unchanged-source comparison.

本文区分源码、真实数值、界面操作和平台发行包验收。最终 Full 使用第十检查点；
受影响组件重新验证，未改组件按源码哈希关联既有证据，不把历史执行写成本次重跑。

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

Runtime source checkpoint 10 was frozen as 875 files; its private input ZIP has
SHA-256 `bc8f1007f0e30f6e8198f1aaa2324706aa407d9b430a842e4664b2789a52b839`.
The final source archive adds current documentation, illustrations and the macOS
resource lock. Runtime code stayed unchanged after this freeze.

The broad host/core matrix below ran at checkpoint 9. Checkpoint 10 reran all
91 PRO and 45 base Viewer tests on Linux, plus 5 observation-date and 28 science
tests. Windows reran its five affected workspace tests with actual colon-delimited
keys and the same 5+28 Python tests, then rebuilt its native Full executable.
An independent AST comparison confirms Gaia changed only one rejection diagnostic.
Unchanged core/server/transport/spectral checks are inherited by full file hashes;
the corresponding Full native and WASM release builds compile the changed UI.

| Matrix | Actual result |
| --- | --- |
| Linux/TNT Full host | 36/36 tests; all-features and no-default checks pass. |
| Windows Full host | 35/35 tests; desktop-only check passes. |
| Headless PRO host | Linux 35/35, Windows 34/34; matching-source evidence includes actual copy cancellation. |
| Viewer PRO / base | Linux source10: 91/91 and 45/45; Windows source9: 91/91 and 45/45, plus source10 affected workspace 5/5. Base test-name sets match. |
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
and expanded PRO tools: 78 selected/open/task cases produced 140 PNGs. A separate
repeat retained response bodies and had zero network errors. Visual review is
independently recorded for all 39 English wide-window pages and six Chinese
narrow-window examples (45 images linked by SHA-256). These counts establish
rendering/inspection, not 78 successful scientific calculations.

Controlled reproduction distinguished a correctly rejected old-role heartbeat
from a real fresh-epoch race when PRO was first entered in an iframe older than
45 seconds. Source-9 delays the first heartbeat and initializes the Gaia restore timer.
Actual delayed/reacquire checks observed the first PRO heartbeat at 45.027s with
no fresh-epoch rejection, and restored Gaia's at 30.487s with HTTP 200. The first
idle probes timed out under on-demand frame timing; retained retries used real
mouse movement over the title to sustain frames, without scientific commands.
Those original failures and the correctly rejected old epochs remain recorded.

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

macOS acceptance must come from real macOS runners. An app startup probe is not
full interactive GUI testing; an ad-hoc signature is not Apple notarization.
JupyterLab 5, Linux/Windows ARM, Linux musl, Safari, actual X2Go forwarding,
production PSF photometry and paper-ready scientific calibration are outside
this release's acceptance scope.

## Final GUI and packages / 最终界面及发行包

The actual checkpoint-9 installed-wheel GUI performed **Nearby observations →
Search → CDS/P/DSS2/red → Generate FITS cutout and add**. The server saved the
512×512 result beside the original; a second viewer tab appeared automatically.
The saved FITS is 529920 bytes with 262144 finite pixels and SHA-256
`c690296c95f1dc5c5300480a3bbbb8bb9f67130fed08200acac05d6f4fd3b5cc`.
Astropy and the installed Rust reader agree. Reselecting the original retained
its aperture, original tab and original hash. The cutout's area ratio is
1.0000590893 and actual overlap approximately 1; it is a resampled HiPS cutout.
The 149-member evidence manifest and ZIP CRC were independently verified.

That real GUI run exposed a missed integration bug: the actual tool key is
`archive_search:`, while the 600px default/migration tested only the bare name.
Source10 fixes matching without mixing saved subtask sizes. Its actual fresh
file-list double-click → Catalogues → Nearby observations used 600px with no
height injection. A real drag to 420px, tool switch and return retained 420px;
the actual workspace Reset restored 600px and cleared overrides. Twenty recorded
actions passed. The source9 workflow's manually set height is not promoted to
default-height acceptance.

The checkpoint-9 Linux Gaia button completed network/date processing but rejected
a solution with fit RMS≈0.824″ and holdout RMS≈1.728″ at a 3″ limit. Source also
requires maximum holdout residual≤6″, so that guard explains the rejection; its
exact maximum was not retained. Source10 now reports all three values and limits.
The algorithm/thresholds were not weakened or rerun to obtain an accepted picture.
This is distinct from the earlier actual Windows accepted example above.

Thirteen new module screenshots use the actual installed Full10 wheel and real
M67/SDSS input, with three genuinely in Minimal mode. Each image was visually
reviewed; the parent independently checked representative aperture, WCS,
archive and spectral pages. The original M67 WCS readout waited for its real
asynchronous response, not an injected Gaia solution. The gallery demonstrates
configuration and real basic Rust aperture calculations, not thirteen successful
external scientific jobs. [Image provenance and byte hashes](../images/v0.1.6/README.md).

Final Linux portable Full was built against glibc 2.28, repaired and inspected
with auditwheel in 345.46 seconds, with no source drift. Its installed wheel
uses its packaged host/web/Python resources without source PYTHONPATH. The parent
checked its native/WASM/JS hashes and all 18 Python adapters against source.
A separate authenticated Jupyter trial uses the workspace root; the existing
two Jupyter servers, their process start identities and their kernel/session
identities were preserved. The final trial was actually opened in the in-app
browser, loaded M67 and entered PRO, then returned to Minimal for the user.

Windows source10 Full rebuilt natively and packaged matching shared resources.
The 610 wheel RECORD entries, 34 shared resource files, 542 original license
files covering 240 Windows native dependencies, Unicode/space extraction, PE
0.1.6.0 and privacy checks passed. Its portable contains 681 files, including
AgentWorkLog. Three explicit documentation-only payload overlays replace stale
validation text and provide current notes/logs; they do not alter frozen runtime.
The final portable was installed into a separate versioned directory, preferences
copied, and only this task's identified Start-menu shortcut updated. Its native
window remained alive/responding; the preceding task-owned source9 window was
retired. Older user programs, defaults and scientific-file icons were preserved.
No final10 Windows native mouse/keyboard acceptance or clean positive Setup
install/uninstall is claimed. The source9 actual Windows Jupyter actions above
remain source-matched shared-UI evidence, with layout separately tested on Linux.

Both Mac architectures were natively built and checked on real macOS 15 in
[run 39180857987](https://github.com/minglinwang86/FitsViewer/actions/runs/39180857987),
at commit `cd2371a76055c7895310eeccf1b9f2b6dac2e5f6`. The locked Linux wheel supplied
shared resources; each runner compiled its own native app. Relocation, CLI,
icons/ad-hoc signature, external-Python fver, real public HorseHead FITS values
against Astropy, authenticated API and external worker termination passed.
Native window startup/screenshots succeeded, followed by owned-process cleanup.
The parent independently checked both downloaded ZIP CRCs/hashes, Mach-O CPU,
plist and absence of scientific-file associations. Each matches 29 shared runtime
files (Python line endings normalized only) and verifies 251 native dependency
records with 462 original license-text records. This is startup acceptance,
**not interactive Mac GUI acceptance or Apple notarization**.

Final test-owned views/workers/artifacts were released individually. The user's
deliberately retained Jupyter/base view and Windows app are intentional services,
not abandoned test processes. This does not establish a universal no-leak/RSS
guarantee for every driver, backend, third-party package or arbitrarily long run.

### Final asset checksums / 最终产物校验

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| `fitsviewer-0.1.6-py3-none-manylinux_2_28_x86_64.whl` | 10,525,142 | `a5c8eb059b2be3503c95d8c0816eceb59911f7c37d8e53e5034e1951a37b9010` |
| `fitsviewer-0.1.6-py3-none-win_amd64.whl` | 12,622,960 | `65c482b27e0617be5b83080d916ee4a97c0e19971f550902b77bdf3f032dff1d` |
| `FitsViewer-0.1.6-windows-x86_64-portable.zip` | 31,156,274 | `5e927666fe6f07d90a69a2d9ea6b787666cd3e9b25b1b0026574f5664fa5d468` |
| `FitsViewer-0.1.6-windows-x86_64-setup.exe` | 26,231,245 | `3690a6a93c266e277f7a357c67fb44dc323da33631406d0b2298cd2612684ba8` |
| `FitsViewer-0.1.6-macos-arm64-portable.zip` | 10,430,934 | `901ed3725e98408b7e53ba367d7c3dbfa30fe2e5a2f5afdbd412bdbb53003835` |
| `FitsViewer-0.1.6-macos-x86_64-portable.zip` | 10,790,382 | `cd3394b393596dec6f0f03c2a10106913b99ff510a1d7ac013508f94b08abe18` |

Shared final WASM SHA-256:
`9829e237ef74c2f1812263c8e3984b6ade7c8132108c556dd3aeeeabbf2fe93a`.
The final source archive includes the current guides, AgentWorkLog and screenshots.
Use the release's `SHA256SUMS` for that archive and all six platform downloads;
the private frozen input ZIP above is a separate engineering checkpoint.

本次只发行 Full（包含极简与 PRO），没有独立 Mini。现有科学结果仍只建议作检查参考；
Mac 为临时签名、未公证，也未声称完整原生交互验收。各平台新执行、沿用证据、预期的
科学质量拒绝及明确未测范围分别记录；不能把一次查询成功当成档案服务永久可用。
