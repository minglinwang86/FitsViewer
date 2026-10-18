# Internal validation checkpoint 1018001

Validated on 18 October 2026. This is a source/testing checkpoint, not a published
release or a certification of every operating system. No installers were produced
for this acceptance run. Scientific Python packages and TeX are not bundled.

## Changes covered

- Framed image/spectrum PRO workspaces, movable/resizable tab groups, edge
  alignment, collapsed rails and saved window states. Minimal popup geometry is
  stored by document type without initializing PRO.
- Uniform toolbar controls and spacing, a compact whole-name task selector in
  narrow windows, filename-preserving path display, fixed two-row pixel status
  and optional explanatory text behind the relevant `…` controls.
- Explicit uncertainty HDU and sigma/variance/ivar selection, original per-HDU
  headers, measurement units/provenance and visible spectral fit diagnostics.
- Valid legacy DSS plate WCS support. Replacing a WCS clears the old DSS/TPD
  model, and aperture templates retain the self-contained polynomial cards.
- Bounded jobs/artifacts, document and tool leases, stale-response rejection,
  derived-data pins and deterministic worker/tree cleanup. Windows cache paths
  use extended Unicode paths. Jupyter proxy children observe their parent and
  stop their workers when Jupyter exits.

The final v3 UI differs from the validated v2 runtime only by removing a redundant
photometry success message and its translation. The result, unit, uncertainty HDU
and measurement flags remain visible. Server, core and Python inputs did not
change. Both v3 WASM profiles were rebuilt; the HST result remained identical in
actual English/Chinese PRO and true Minimal interfaces.

## Actual execution

| Environment and scope | Result |
|---|---|
| Linux x86_64, Debian 12 isolated container | Core 33/33; PRO host 9/9; real-science CLI 50/50 |
| Linux packet/worker/HTTP lifecycle | 85/85; real process identities and POSIX exit checks |
| Linux supervisor and abrupt parent death | 4/4, including a real worker and a real blocked FITS packet writer |
| Linux native desktop under Xvfb | 4/4: real DSS coordinates, HST SCI/ERR photometry, close/reopen and active-worker window close |
| Linux JupyterLab 4.5.11 | 6/6: isolated kernels, Chinese FITS/FZ/spectrum double-clicks, executed Notebook `show()`, active-worker Jupyter shutdown |
| Windows x86_64 headless/runtime | 72 protocol/worker checks; 8 Unicode/long-path checks; v2 HTTP 13/13 |
| Windows Chromium with paired Mini/PRO | v2: 12 real view loads in four cycles; earlier v1: 30 loads in ten cycles |
| Windows Jupyter parent shutdown | Real HST WCS worker and viewer left active until Jupyter shutdown; owned processes exited naturally |
| DSS overlay numerical comparisons | 10/10: raw DSS, self-contained TPD, replacement TAN, SIP and protected observation metadata |
| Shared UI inspection | 75 PNGs individually inspected, including actual expanded photometry, Gaia, relative calibration, catalogues and spectrum tools |

Linux Jupyter ran on Linux; the browser was Windows Chromium through a private
loopback test relay. Native Linux tests used an actual desktop binary and Xvfb,
without substituting the web viewer. The native binary predates only the final
cosmetic success-message removal. Windows Jupyter parent-lifetime validation
used the new runtime with an explicitly recorded older UI; final v3 shared UI
and Linux Jupyter were verified separately.

All scientific originals retained their hashes. Failed QA premises and setup
attempts were preserved separately: a text spectrum already had named variance;
an original SCI header already contained ROOTNAME; a Notebook with an unsaved-work
dialog was still open; and one private runtime initially omitted its Python
adapters. These were corrected in the test setup rather than called product fixes.

## Resource observations

Every closed v2 browser cycle had zero iframe/WASM instances, zero registered
view/tool owners, zero artifacts and a stopped worker. Observed closed-host RSS
was 15.88–16.36 MiB for PRO and 15.11–16.20 MiB for Mini. All recorded owned
processes exited; the final successful runs needed no fallback termination.

These bounded cycles do not prove unlimited-duration memory stability or require
the browser/GPU allocator to return reserved memory immediately. The runtime
does not expose a jobs counter; unavailable metrics were not reported as zero.
See [resource lifecycle](RESOURCE_LIFECYCLE.md) for ownership and expiration rules.

## Real astronomy workflows and limits

The private test inputs include SNfactory SN 2011fe, SDSS QSO/galaxy spectra,
HST NICMOS SCI/ERR, a real M67 photographic scan, CALSPEC and a PS1 Q2237 field.
They exercise source uncertainty, continuum windows, equivalent width, velocity
display, bounded profile fits/residuals, aperture errors, HDU metadata and WCS.
The lossless Rice M67 fixture is an explicitly labelled storage representation
of the real scan, not a separate observation. Scientific datasets are not shipped.

A blend minimum and an assumed reference wavelength do not identify a species
or establish a photospheric velocity. A converged Gaussian is not an adequate
AGN model by itself. The M67 scan is not linear calibrated CCD photometry, and
the PS1 field does not validate resolved lens modelling. Instrument DQ bits,
absolute photometric calibration, time-series analysis, PSF/lens/AGN physical
models and blind all-sky solving remain external or require additional work.

See [spectral analysis](SPECTRAL_ANALYSIS.md), [scientific tools](SCIENTIFIC_TOOLS.md)
and [image layers](IMAGE_LAYERS.md) for the supported operations and assumptions.

## Component sizes and remaining platform coverage

Final optimized development WASM assets are 3,973,098 bytes (Mini) and
4,725,491 bytes (PRO). The shared common Chinese font is 677,956 bytes. These
are component sizes, not wheel/installer download sizes; no scientific Python
environment or data is included in them.

An actual X2Go session, all clipboard/IME sources, a current Windows native
desktop release build, other Linux distributions/architectures and macOS native
execution require separate acceptance. The Windows stretch test executable
failed before its harness started; that is not recorded as a passing Windows
test. The complete core suite, including stretch, did pass on Linux.

For future changes use [the architecture map](ARCHITECTURE.md),
[the AI development guide](AI_DEVELOPMENT_GUIDE.md) and
[the build profile instructions](BUILDING.md). Preserve the distinction between
source checks, runtime tests, visual inspection and release artifacts.

## 中文摘要

1018001 为内部测试检查点，未发布或制作安装包。Linux 已完成核心、服务端、真实科学
输入、进程生命周期、原生窗口与 Jupyter 实际验收；Windows 已完成运行时、浏览器和
Jupyter 父进程退出验收。测试期间发现的 DSS 坐标替换、误差来源、长中文路径和进程
退出问题已修复，最终中英文 UI 另有逐图检查。

反复关闭测试未发现继续保留的查看器、临时数据或自有 worker；有限测试不等于长期
无泄漏保证。真实 X2Go、全部输入法来源、Windows 原生发行构建及其他平台仍需独立
验收。测光、谱线、速度和拟合结果保留单位、来源及科学假设，不代替完整科研管线。
