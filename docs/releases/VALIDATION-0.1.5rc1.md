# Full 0.1.5rc1 validation / 全功能版验收

This report separates current shared-source checks, platform execution and scientific
limitations. It does not certify every operating system, graphics driver or research use.

## Shared source and current Linux/Jupyter UI

- Full Rust tests: 73/73. The existing Mini-feature checks: 35/35. These test the
  mode/module boundary; no Mini-only release artifact is distributed.
- Chinese/English image/spectrum toolbars, narrow windows, Unicode paths, aperture
  selection/deletion/resizing, stable bottom readout and compact title bars were checked.
- Coordinate and photometry popup sizes survived repeated opening, closing, saving
  and refresh. PRO fold, float, move, dock and close interactions were exercised.
- Public M67 FITS, lossless tiled FITS and SDSS spectrum loaded through HTTP and
  native Linux/Xvfb. Notebook IFrame generation was checked.
- The current shared WASM digest is
  `d0db196e6112950efd272e125b9651b6f752fdbbf040e7711cd2c3e21f3475d5`.

## Linux Full package

- Release native code was built for the glibc 2.28 baseline; auditwheel 6.8.2
  verified and repaired the `manylinux_2_28_x86_64` tag. Strict Twine metadata checks passed.
- The actual packaged binary ran version/build-info, a public NASA image and an
  SDSS spectrum through glibc 2.28 and 2.31 loaders, including resource discovery
  outside the checkout. This is loader/backend acceptance, not an entire old-desktop test.
- A clean environment without Jupyter or scientific packages imported the package,
  ran the CLI and opened a native Minimal image window under Xvfb; it exited normally.
  A separate Python 3.10.12 environment also passed package import, Full build-info
  and real SDSS CLI inspection without Jupyter/science dependencies.
- Installing into an environment with JupyterLab 4.4.0 and no server-proxy preserved
  Lab 4.4.0, installed the missing proxy, enabled binding and passed `pip check`.
- Actual JupyterLab 4.4 browser checks covered 37 reference API cases, Unicode
  FITS/gzip/FZ double-click opening, pasted quoted paths, image/spectrum interactions,
  several spectrum column layouts and an executed Notebook `show()` cell.
- The status bar retained its height at 850, 1100 and 1400 px. Four cycles of three
  opened/closed documents each returned the live WASM instance count to zero;
  no JavaScript errors were recorded.
- A pinned scientific artifact survived PRO release while still referenced;
  releasing its view removed it. Private Jupyter shutdown stopped its owned worker.

## Windows Full package

- Actual host: Windows 11 25H2, build 26200.9711, x86_64; QA Python 3.13.5 in
  isolated environments. Windows 10 was not executed in this acceptance run.
- The packaged native binary passed 37 reference packet cases and six actual
  image/spectrum/Unicode/compression window-open-and-exit cases. A Chinese PRO
  window, font and application icon were visually inspected.
- Setup installed into a directory containing Chinese characters and spaces;
  installed payload files matched the prepared payload byte for byte. Start-menu
  shortcuts pointed to the program icon; data-file ProgIDs had no DefaultIcon.
- An existing JupyterLab 4.6.4 environment without server-proxy was rejected when
  dependency installation was disabled. Enabling it installed the wheel's Jupyter
  extra and activated both server extensions. The Conda-folder input path was
  exercised with an isolated venv, not a real Conda installation.
- Silent rejected installation returned exit code 1 without a blocking dialog or
  partial registration. Three uninstall cycles restored the previous fallback
  registrations. Protected UserChoice and generic `.gz` defaults stayed unchanged;
  the test's installation state, logs and shortcuts were cleaned up.
- `.fz`-only protected defaults were checked with the actual Pascal decision
  function in an isolated test fixture; the test did not write real UserChoice keys.

## Packaging and documentation

Checksums are published separately in SHA256SUMS to avoid self-referential package
hashes. Public scientific screenshot sources and the distinction between executed
operations and configuration-only examples accompany the figures.

The release does not bundle Astropy, NumPy, Specutils, a Python interpreter, TeX
or observational datasets. Application dependencies and font licenses accompany
the packages. Repackaging after document edits preserves the tested native,
WASM, frontend and Python runtime payloads; only the reviewed documentation and
its package metadata/checksums change.

## Scientific and lifecycle boundaries

- Existing independent array/coordinate/uncertainty checks use real public data
  alongside controlled fixtures. Source input hashes remain unchanged.
- The Gaia/WCS cache has document/HDU/generation identity checks and bounded size;
  changed coordinates invalidate dependent registrations and stale jobs.
- Owned workers, temporary results and renderer/document references are checked
  separately from process RSS. Browser and allocator caches may retain memory.
- Source finding is simple centroid-based detection, not PSF photometry. Layer
  matching is relative count scaling, not a calibrated zeropoint. Visualization
  reprojection is not flux conserving.
- Spectral EW/pEW/fit uncertainties depend on valid input errors and stated
  assumptions. Continuum choice, calibration, redshift and LSF systematics remain.
- Current UI screenshots of online configuration do not demonstrate a successful
  live Gaia/CDS/VO service response. Remote SAMP needs an actual external Hub/client.
- No claim is made for all Linux distributions, ARM/musl, Safari, macOS native
  packaging, actual X2Go forwarding, all clipboard sources or JupyterLab 5.

中文：验收区分共享源码、实际平台运行与科学解释。模式／布局、真实图像／光谱、资源回收
和独立数值参考检查不能代替特定观测的科学验证；不把编译成功、配置截图或进程 RSS
立即下降当作完整正确性证明。最终发行文件的校验值单独公布。
