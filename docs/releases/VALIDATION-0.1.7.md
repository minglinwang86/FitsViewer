# 0.1.7 development validation / 开发验证记录

Requested on 2026-10-27; implementation and checks continued on 2026-10-28.
This is a source-development checkpoint, not a published cross-platform release.

## 2026-10-29 Linux candidate / 最新 Linux 验收

Earlier counts below describe the October 28 checkpoint. The current local Full
candidate includes the subsequent fixes, six languages and Python/PyEC manual selection.
Windows/macOS final builds and public publication are still pending.

- Six dictionaries share **918 keys**. New-key checks passed; existing six-language
  narrow-layout and scientific numerical evidence was reused where code did not change.
- Focused checks: Python interactive/client **18**, host selection **6**, viewer selection
  **4**, spectral analysis **8**, workspace **5**, PRO result presentation **7**, locale
  checks **4**. JavaScript selection bridge passed six controlled scenarios. These
  overlapping module checks are not a claim that the entire workspace was rerun.
- Imported actual PyEC `snoopylib`, opted into `make_pyec_xparun`, displayed real M67,
  selected with X/mouse, finished with Q and received matching one-based coordinates.
  The same browser then opened another FITS at HDU 1 and returned explicit Esc cancellation.
  Source files and PyEC source hashes stayed unchanged. This exercises the interface,
  not a complete seeing/PSF/photometry pipeline.
- The packaged wheel was installed into a fresh task-local Jupyter target. Actual Lab
  double-click, aperture mouse input, measurement and spectral-result display passed.
  Private backend header authentication and proxy access passed; no backend credential
  appeared in its browser URL. The test Jupyter/backend processes exited.
- Screenshots were inspected: aperture values/units/uncertainty/Copy JSON are visible
  in a right-side first-use popup; spectral primary metrics and validity failures are
  visible with full warnings expandable. A settled actual Model fit tab click starts
  its own scroll position at the top, without the previous task's warnings.
- Native Linux opened real image/spectrum data, forwarded into the same window and
  closed two windows after an actual Exit all click. Registry entries and owned GUI/
  Xvfb processes reached zero.
- The earlier 2 GiB sparse synthetic test passed Minimal→PRO→Minimal with identical
  original float64 readouts, bounded tile traffic and cleanup. It is not a benchmark
  of all real observational files. The exact originally reported failing template
  remains unavailable.

Artifact: `fitsviewer-0.1.7-py3-none-manylinux_2_28_x86_64.whl`, **11,191,023 bytes**.
SHA-256: `085e8072e0535560ca2b52698cc0d7f85f664f96bbdd1def5445a808fcde5eb8`.
The actual ELF requires at most GLIBC 2.28; auditwheel repair passed. Native/WASM
payload hashes matched the tested profile, and 28 packaged runtime text files
matched current source. Detailed local evidence is `FINAL_VALIDATION.json`.

One early selection-UI filter executed zero tests; it was not accepted as coverage.
The final filter executed all four tests. An initial browser-bridge check failed
because Node VM objects have different prototypes; the corrected test compares their
serialized values. Both earlier results were preserved.

## Implemented behavior

- Physical-value histograms have editable Low/High inputs and directly clickable/
  draggable markers. Right-click sets High. Values must be finite and ordered;
  neither the histogram nor display limits modify source pixels.
- **Exit all** requests graceful exit of registered same-user FitsViewer windows.
  Paired remote helpers participate in exit but cannot receive local-file forwarding.
  Browser teardown runs after the active WASM render callback; Jupyter closes only
  FitsViewer widgets and keeps its server and other documents running.
- **Open new images in the same window** is a remembered setting. Desktop opening
  waits for an acknowledgement and falls back if no compatible viewer responds.
- Native two-dimensional images use asynchronous metadata/preview/tile access.
  Minimal mode can use 16-bit display samples. **PRO retains original-bit-depth
  tiles**, and precision changes invalidate textures and late responses. Compact
  previews are not reused as PRO image-layer data.
- RICE/GZIP-compressed FITS images can decode the source compression tiles needed
  for a view instead of allocating the complete plane. A source compression tile
  has a 16 MiB display decode/input budget. Original dither order is retained.
  Original-pixel queries and scientific operations remain separate.
- Native spectra retain the existing full reader instead of inheriting the HTTP
  viewer's 20 MB transfer limit. Uncompressed original headers can be read without
  applying the full-array file limit to a header-only request.

## Evidence and limits

- Linux core regression: **42 passed**, including edge/scale/mask checks, GZIP_1/
  GZIP_2 compressed ROI comparisons and a sparse 1.28 GB header/tile case.
- Shared egui regression: **94 passed**, including histogram mouse interaction and the existing bilingual
  Minimal/PRO layouts pass. Additional tests cover precision-cache replacement
  and the native-spectrum full-reader fallback. Counts are retained in the build logs.
- Host regression: **40 passed**, covering acknowledged Unicode/HDU forwarding, exit ownership, remote-helper
  capability separation and existing cancellation/artifact/process lifecycles.
- `qa/verify_large_image_transport.py` exercised a real authenticated HTTP server
  against independent Astropy/CFITSIO reads for five datasets: float64 FITS, a
  6000×6000 integer RICE image, floating RICE and GZIP_2 images, and an existing
  real SDSS download. Original tiles/readouts matched; compact-tile error and masks
  stayed within the quantization bound; input hashes stayed unchanged. The owned
  test server was reaped.
- Browser window-control source tests, Jupyter TypeScript checking, WASM compilation,
  Python service/download tests, module-selection checks and Chinese glyph coverage
  are recorded separately. Source tests are not a complete visual/browser acceptance.

The exact file originally reported as an unopenable template was not supplied.
The real download checked above opens, and the previously definite full-plane
limits for large internally compressed images have been addressed. This does not
establish support for every archive format, cube, compression algorithm or corrupt file.

Windows native compilation/testing encountered toolchain path and temporary-file
permission failures; retained logs distinguish these from application failures.
There is no new Windows or macOS runtime-acceptance claim or installer in this
checkpoint. Published 0.1.6 packages and tags remain unchanged. A future 0.1.7
release must rebuild generated viewer/Jupyter assets and update the macOS resource
locks against its matching Full Linux asset; it must not reuse 0.1.6 generated assets.
