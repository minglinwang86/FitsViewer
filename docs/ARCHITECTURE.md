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

The main data flow is file → Rust core → shared image/spectrum data → UI texture or
plot. Scientific jobs pass a bounded JSON request through `science_client.rs` and
`crates/server/src/science.rs` into `fitsviewer_science.py`. Results are checked against
the current document/generation before changing overlays or derived state.

`lib.rs` coordinates the document lifecycle. `ribbon.rs` owns the top task/options
rows, `controls.rs` preferences, `coordinates.rs` WCS/Gaia, `regions_ui.rs` apertures.
`ui_style.rs` owns shared control geometry, interaction colors and popup/window
appearance for both Mini and PRO, without changing scientific plot or image colors.
PRO-specific responsibilities are in `pro.rs`, `multiview.rs`, `image_layers.rs`,
`spectral_lines.rs`, and `spectral_analysis.rs`. Basic and full spectrum rendering
reuse geometry from `plot_geometry.rs`.

`workspace_ui.rs` owns PRO docking, floating groups, panel frames and layout JSON;
it does not own the scientific controllers. `document_lifecycle.rs` centralizes
file replacement, cached selection, close and artifact references. `header_ui.rs`
reads one original HDU header without merging it with another HDU.

`window_layout.rs` is a base module for Minimal-mode popup geometry, open and
collapsed state. Its stable keys distinguish image, spectrum and empty contexts;
it does not instantiate PRO controllers. Preferences store both kinds of layout,
while imported session settings take precedence over the current popup cache.

The host bridge separates `science_sessions.rs` (bounded document/role leases),
`science_process.rs` (owned process trees and their stable creating thread), and
`science.rs` (serialized protocol and temporary-artifact ownership). Pure Rust
`measurement.rs` serves both native and HTTP header/photometry requests without
starting Python. See [resource ownership](RESOURCE_LIFECYCLE.md).

`server/src/supervisor.rs` observes the Jupyter server's lifetime independently
of third-party proxy cleanup. It observes only; it never signals the parent.

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
- Closing a document releases its view/worker owners; collapsing a panel must keep
  active task polling and cancellation available. Mode changes do not reuse stale jobs.
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
