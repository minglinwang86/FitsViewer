# FitsViewer architecture / 代码架构

This describes the source architecture, including the prepared module selection. It is
not a claim that every new profile has completed release or platform validation.

## Responsibilities / 职责边界

| Layer | Authoritative sources | Owns |
|---|---|---|
| Scientific core | `crates/core/src/` | fitsrs reading, physical samples, compressed images, spectra, stretch, regions and basic aperture numerics |
| Shared native/WASM UI | `crates/viewer/src/` | document/view state, egui controls, display textures, coordinates, optional PRO workspace |
| HTTP and native host | `crates/server/src/` | authenticated file/API access, bounds, native entry, owned Python worker and jobs |
| Environment adapters | `web/python/` | lazy calls into the selected Python environment; no embedded scientific runtime |
| Python entry/integration | `jupyter/fitsviewer/` | CLI, target environments, settings, server proxy, Notebook URL/IFrame |
| JupyterLab frontend | `jupyter/src/index.ts` | document factory, file extensions and lifecycle; embeds the shared viewer |
| Packaging selection | `config/build-modules.json`, `scripts/build_profiles.py` | source features and selected resources from one checkout |
| macOS bundle | `macos/launcher.c`, `scripts/build_macos_full.py` | native app entry, locked shared resources, target-specific license collection and portable signing |

The local data flow is file → Rust core → shared image/spectrum data → UI texture or
plot. HTTP views first negotiate a manifest. A decoded selected-HDU packet up to
`20_000_000` bytes is transferred once; a larger supported two-dimensional image
uses a numeric preview and visible tiles. `core/src/remote.rs` owns the protocol,
`server/src/remote_data.rs` owns preparation/seek access, and
`viewer/src/remote_image.rs` owns progressive display. Original values and science
requests stay separate from sampled display data. Scientific jobs pass a bounded JSON request through `science_client.rs` and
`crates/server/src/science.rs` into `fitsviewer_science.py`. Results are checked against
the current document/generation before changing overlays or derived state.

In 0.1.7, `server/src/native_images.rs` exposes that same bounded store to the
native UI through an in-process callback, rather than reading the full array before
creating a window. `core/src/display_tile.rs` can encode Minimal-mode display tiles
as 16-bit samples with a physical-value scale/offset and an invalid code. PRO requests
original-bit-depth tiles. Scientific files, exact pixel reads and measurements are
separate. `compressed_reader.rs` can decode only the source compression tiles needed
for a view, retaining the original per-compression-tile dithering sequence.

`server/src/native_windows.rs` maintains private same-user window mailboxes for
acknowledged open requests and graceful exit. Only local-file viewers accept local
open forwarding; paired remote helpers can participate in exit without gaining local
file capabilities. No process-name kill or Jupyter shutdown is used. Browser windows
use an origin-scoped channel, and the Jupyter adapter closes only FitsViewer widgets.

The shared translations live under `web/locales/`: one registry and one JSON
dictionary per language. `crates/viewer/src/i18n.rs` embeds those same files for
native/WASM use; `web/locales/index.js` loads them for the browser shell. English
message keys and unknown values fall back unchanged. Dictionaries are parsed on
selection rather than all at startup. Adding a language must preserve placeholders,
scientific identifiers and units, and pass both dictionary and narrow-layout checks;
see `web/locales/README.md`. Do not add another independent translation table inside
the HTML page or a tool controller.

`lib.rs` coordinates the document lifecycle. `ribbon.rs` owns the top task/options
rows, `controls.rs` preferences, `coordinates.rs` WCS/Gaia, `regions_ui.rs` apertures.
`ui_style.rs` owns shared control geometry, interaction colors and popup/window
appearance for both Mini and PRO, without changing scientific plot or image colors.
`image_transform.rs` supplies the same forward/inverse pixel transform to textures,
apertures, picking, WCS readout and overlays. Display rotation does not resample the
scientific array. `image_display.rs` draws the scale/compass and rotation controls;
`environment_ui.rs` remains a base controller for external Python settings.
PRO-specific responsibilities are in `pro.rs`, `pro_forms.rs`, `pro_points.rs`, `multiview.rs`, `image_layers.rs`,
`spectral_lines.rs`, and `spectral_analysis.rs`. Basic and full spectrum rendering
reuse geometry from `plot_geometry.rs`.

`workspace_ui.rs` owns PRO docking, floating groups, panel frames and layout JSON;
it does not own the scientific controllers. `document_lifecycle.rs` centralizes
file replacement, cached selection, close and artifact references. `header_ui.rs`
reads one original HDU header without merging it with another HDU.

`window_layout.rs` is a base module for Minimal-mode popup geometry, open and
collapsed state. Its stable keys distinguish empty, Minimal image/spectrum and
PRO image/spectrum contexts;
it does not instantiate PRO controllers. Preferences store both kinds of layout,
while imported session settings take precedence over the current popup cache.

The host bridge separates `science_sessions.rs` (bounded document/role leases),
`science_process.rs` (owned process trees and their stable creating thread), and
`science.rs` (serialized protocol and temporary-artifact ownership). Pure Rust
`measurement.rs` serves both native and HTTP header/photometry requests without
starting Python. See [resource ownership](RESOURCE_LIFECYCLE.md).

`server/src/supervisor.rs` observes the Jupyter server's lifetime independently
of third-party proxy cleanup. It observes only; it never signals the parent.

The Jupyter adapter creates a private per-proxy backend credential. Its environment
and `request_headers_override` callbacks share that credential; it is not added to
browser URLs. Jupyter-mode host routes require the private header. Standalone
authentication and the separate fver capability namespace retain their own boundaries.

Manual star picking is an independent bounded session in `server/src/selection.rs`.
`viewer/src/selection_ui.rs` uses the existing original-image transform and separate
markers, without creating photometry apertures. `web/index.html` follows source/HDU,
writer identity, revisions, cancellation and actual display-setting acknowledgements.
The stdlib clients `jupyter/fitsviewer/interactive.py` and `pyec.py` expose final-list
selection and a narrow incremental DS9-style adapter respectively. Browser/Jupyter
selection is implemented; native selection IPC and a complete XPA server are not.
See [the manual selection contract](MANUAL_STAR_SELECTION.md).

The independent SSH entry is `jupyter/fitsviewer/remote.py` / `fver`. It uses a
separate broker namespace, rather than a Jupyter server or its master credential.
`remote_broker.rs` owns explicit-file grants, one-use bootstrap/pairing tickets,
target queues, scoped scientific owners and delivery receipts. `remote_native.rs`
is a desktop-only fixed stdin connection entry; `web/remote.js` handles browser
bootstrap, following a target and bounded image fetches. The broker and numeric
transport belong to **base**, independently of the optional PRO tools.

`server/src/environment.rs` probes/selects the external interpreter; executable
symlinks are preserved so a virtual environment does not turn into its base
Python. `fitsviewer_environment.py` checks actual imports and resolves missing
packages before installation, rejecting changes to installed distributions.
Dependency jobs use the base `view` role, including status/cancel polling. A close
in another window must not kill an active installation or discard its undelivered
result. See [resource ownership](RESOURCE_LIFECYCLE.md).

## Build modules / 按模块选择

| Module | Includes | Selection |
|---|---|---|
| `base` | FITS, images/basic spectra, basic apertures, WCS/Gaia, HTTP/CLI | Always included |
| `pro` | PRO UI, layers, advanced spectra and corresponding Python adapters | Rust `pro` feature + resource registry |
| `desktop` | Native egui host and platform desktop entry | Host `desktop` feature; optional eframe/viewer dependencies |
| `jupyter` | Lab frontend, server integration and Notebook embedding | Frontend/package assets; optional external dependencies |

| Preset | base | pro | desktop | jupyter |
|---|---|---|---|---|
| `mini` | yes | no | no | yes |
| `desktop-mini` | yes | no | yes | yes |
| `remote-pro` | yes | yes | no | yes |
| `full` | yes | yes | yes | yes |

Default Cargo features preserve the full developer build. The profile driver always
passes explicit features to **both** the host and WASM. A full package in Minimal UI
mode still contains PRO code; a `mini` build excludes the PRO modules at compilation.
Both keep Gaia/basic photometry. Switching modes does not install missing modules.

The present PRO group is one build unit. Individual PRO tools are separate source
modules and external adapters, but arbitrary per-button linking and runtime binary
plugins are not implemented. This boundary avoids maintaining copied applications.

Shared star detection lives in `fitsviewer_detection.py`; shared TAP operations live
in `fitsviewer_tap.py`. Gaia imports these directly. The PRO adapters re-export the
same helpers where needed. Packaging never generates stripped science-source forks.

Archive discovery lives in `fitsviewer_archive_services.py`; spherical footprint
geometry is isolated in `fitsviewer_archive_geometry.py`. The latter uses a sampled
WCS boundary, spherical area/intersection and conservative rejection of unknown
geometry. It does not infer a footprint from a survey name or a bounding RA range.
`fitsviewer_vo.py` streams and validates chosen products into the owned artifact
cache. Persistent saves belong to the Rust host's `download_save.rs` and are separate
from the cache: closing a viewer must not delete a completed user download.

## Plans, artifacts and installed capabilities

Read a plan without compiling, packaging, staging or starting services:

```bash
python scripts/build_profiles.py --profile mini --plan
python scripts/build_profiles.py --profile full --plan
python scripts/build_profiles.py --modules base,pro,jupyter --plan
```

Only `--execute` starts the profile build. `bash scripts/build.sh` remains an explicit
build entry for existing source installers; add `--plan` for inspection. Build commands
belong in a separately authorized build/test step, not in a source-only preparation.

The driver stages selected assets under `build/profiles/<profile>/`, records
`build-profile.json` and `artifact-manifest.json`, and can pass that directory to the
isolated wheel assembler. It never edits the application's source tree to remove PRO.
Wheel staging lives in `build/python-packages/`; profile outputs in `dist/<profile>/`.

The installed Python API reads its module metadata without importing Jupyter or
Astropy. `fitsviewer build-info` reports it; the Rust host's `--build-info` reports its
compiled features. Omitted PRO operations are rejected in the host and Python bridge,
including background-job entry. A package without Jupyter omits its registration assets.

Python scientific dependencies remain external optional extras. A wheel containing the
`jupyter` module can use an existing compatible environment or install its `[jupyter]`
extra. Target-environment installation is a Jupyter integration command and requires
that module. Base-only custom builds can be installed directly with pip.

## Lifetimes and scientific invariants

- Image layers share `Arc<FitsImage>`; texture/cache ownership is separate from raw
  samples. The 128 MiB multi-view LRU limit does not cap total process/GPU memory.
- A tiled active view has a 40 MiB estimated numeric/texture cache, up to four
  local tile fetches and at most twenty visible tile requests. Background layers
  retain a bounded shared preview while their original metadata remains explicit;
  selecting them reloads the original source. `RawPixels::Unavailable` must never
  be treated as an original array or passed to full-image rendering/photometry.
- The host bounds its preparation cache to 192 MiB and gzip spool storage to
  1 GiB, with sixteen admitted image operations. Ordinary FITS supports seeked
  tiles; gzip requires bounded disk preparation. Internally tile-compressed FITS
  decodes the source blocks needed for the requested samples, with a 16 MiB bound
  on each compressed and decoded source block for display reads. Small full-transfer
  products may still use the bounded decoded cache. These are format-specific limits, not a
  promise to decode arbitrary image sizes. Source revisions are checked again
  after reads; cached display statistics are labelled as samples.
- Closing a document releases its view/worker owners; collapsing a panel must keep
  active task polling and cancellation available. Mode changes do not reuse stale jobs.
- A downloaded FITS is copied beside the original only after artifact ownership,
  role epoch, document, source identity and shared-core FITS checks pass. A unique
  same-directory temporary file is committed without overwriting another file.
  Cancellation/release interrupts the copy and removes only its owned temporary
  file. The atomic commit is completion: later cancellation never unlinks a saved
  user file. Unsupported/unwritable destinations explicitly use the temporary
  artifact instead. File-only remote capabilities cannot request downloads/saves.
- FITS physical values preserve BSCALE/BZERO/BLANK and nonfinite masks. Rendering,
  brightness matching and overlays never replace original measurement samples.
- Pixel coordinates are zero-based internally; displayed pixel coordinates are
  one-based. J2000/world transformations and spectral units must stay explicit.
- Field-star matching estimates a relative display scale. It is not an absolute
  photometric zeropoint. Missing uncertainty remains unspecified rather than zero.
- Spectrum comparisons keep native grids. A display overlay is not a coadd or a
  scientifically defined subtraction. Processing uses the full requested data, not
  the reduced display curve.

For exact operations and scientific limitations, see [API](API.md),
[scientific tools](SCIENTIFIC_TOOLS.md), [image layers](IMAGE_LAYERS.md), and
[spectral analysis](SPECTRAL_ANALYSIS.md). For source changes, start with the
[AI development guide](AI_DEVELOPMENT_GUIDE.md).
