# Public API contracts

The viewer reads ordinary/tiled 2D FITS images and basic spectra. It keeps typed storage values and separates FITS numeric scaling from display transforms. Candidate features are described in [NEXT_RELEASE.md](NEXT_RELEASE.md).

| API | Contract |
|---|---|
| MAX_PIXELS / MAX_FILE_BYTES | 32 Mi pixels and 512 MiB; gzip output is capped during decompression. |
| HduInfo / ImageInfo | HDU index/name/type/axes and image dimensions, BITPIX, BSCALE, BZERO, BLANK, header text. |
| RawPixels | U8, I16, I32, I64, F32, F64 owned arrays. |
| RawPixels::len / is_empty / raw_string | Storage length, emptiness and exact textual storage value. raw_string requires an in-bounds index. |
| FitsImage::physical | Apply BSCALE/BZERO once; None for BLANK, non-finite input or overflow. Requires an in-bounds index. |
| FitsImage::to_packet / from_packet | Length-prefixed JSON metadata plus little-endian typed pixels. Spectra append f64 coordinates and optional standard deviations; metadata identifies these arrays. Reject invalid lengths/types/dimensions. Preserve raw i64/f64 and NaNs. |
| load_file / load_file_with_options | Native-only fitsrs reader, optional zero-based HDU. None selects the first viewable image or basic spectrum. LoadOptions also selects zero-based x/y/error columns; ignore_errors explicitly disables uncertainty columns; out-of-range column indices are errors. Errors include IO, malformed header, unsupported HDU/codec, capacity limit and truncation. |
| SpectrumMeta / SpectrumData | Coordinate/flux labels, source units and column choices; f64 coordinates and optional standard deviations. Recognized variance/IVAR columns are converted to sigma. Missing units are not inferred. |
| LoadOptions::error_hdu / error_kind | A 1D image spectrum can explicitly select a different, equally sized 1D uncertainty HDU. `auto` uses an unambiguous column/EXTNAME; unnamed sources require explicit `sigma`, `variance` or `ivar`. Error columns and error HDUs are mutually exclusive. Metadata preserves the source convention; packet errors are always sigma. |
| load_header | Read one original HDU header, including an empty primary. No SCI/primary merge; headers are bounded to 1 MiB and gzip/input limits still apply. |
| Limits::ALL / name | Available interval modes and their UI labels. |
| Stretch::ALL / name / apply | Available stretches, labels and mapping of a finite normalized value clipped to [0,1]. |
| Statistics::new / bounds | Scan finite physical values; sorted sample capped at 1000. Bounds are valid for constant/all-invalid images. Manual initially uses min/max. |
| zscale | Sorted finite sample, contrast; IRAF-style iterative line fit. Empty input falls back to [0,1]. DS9 numerical identity is not guaranteed. |
| DisplayTone / DisplayTone::default / apply | Display-only bias/contrast; defaults 0.5/1. For finite level/bias and positive finite contrast, apply maps `(level-bias)*contrast+0.5` clipped to [0,1]. The UI bounds bias to [0,1] and contrast to [0.05,20]. |
| render_rgba / render_rgba_with_tone | Finite Low < High, stretch and inversion; RGBA8 in screen row order. render_rgba uses the default DisplayTone; render_rgba_with_tone applies the supplied tone after stretch and before inversion. Invalid pixels stay purple. FITS y is flipped for display. Neither function modifies the image or limits. |
| FitsApp::new / from_image | Initialize an HTTP/path-backed viewer, or a viewer with an already loaded FitsImage. |
| start / stop | WASM entry: canvas, API base URL, file path, optional token and HDU, CJK font bytes, optional initial_mode. Resolve explicit mode before the first frame. Returns a JS error if graphics startup fails. stop destroys the managed WebRunner and its app, textures, frame callback and event subscriptions. Document teardown calls it before detaching the iframe; the page also stops polling and loses its WebGL context. |
| setup_server | Python configuration for the supervised Rust service in Jupyter Server Proxy. |
| fitsviewer console entry | Run the wheel's bundled ELF program with packaged web assets; use writable state scoped to the Python environment. Warm the same environment's Jupyter service when available. |
| CLI management | status/ports inspect live services without starting them; ports set saves per-environment future-start settings; tunnel prints a client-side SSH command. |
| Jupyter CLI | jupyter start reuses a matching linked server or starts a separate one; explicit busy ports fail without stopping another server. jupyter url prints the login token only on request. |
| Environment installer | install selects exactly one Python/prefix/Conda target, preserves venv Python symlinks, validates Python/Jupyter compatibility, and runs target pip. A terminal without a target offers interactive selection. --check is read-only. --default-viewer yes/no selects the Jupyter default and --desktop-default yes/no changes the current user's desktop association. Complete installed resources can reconstruct a platform wheel without the original download. Success output reports the target environment, ports, binding, defaults, and shell-quoted binding commands. Target inspection excludes caller FitsViewer/Python development overrides. |
| Default-viewer commands | defaults status inspects preferences; defaults set --jupyter yes/no saves the per-environment factory preference. Refresh JupyterLab to apply it; Open With remains registered. --desktop yes registers the native launcher, and no restores recorded previous defaults. |
| Desktop commands | desktop install registers per-user FITS MIME types and a launcher using the selected environment's absolute fitsviewer path; --no-default keeps an Open With entry. desktop status inspects; desktop remove restores previous defaults and removes owned files. Requires xdg-utils/shared-mime-info. |
| Jupyter server extension | Automatically bind the proxy command to ServerApp.root_dir and base_url. Record only the PID/runtime directory for CLI discovery; authentication stays in Jupyter's own runtime file. |
| show | Python show(path, hdu=None, height=650, base_url=None, mode=None, view=None) returns IPython IFrame; invalid height/HDU raises ValueError. |

HTTP: GET /api/health; GET /api/image?path=...&hdu=... (optional zero-based xcol/ycol/ecol and no_error=true for spectra); POST /api/open with JSON path/hdu and optional session; GET /api/requests?after=... . Standalone APIs require the runtime token. Jupyter mode uses authenticated same-origin proxy access.

Spectrum image queries additionally accept zero-based `ehdu` and
`ekind=auto|sigma|variance|ivar`. Sessions retain `view.load_options`, including
the uncertainty mapping, rather than silently returning to automatic columns.

The 0.1.5rc1 Full release includes the following bounded interfaces:

| Interface | Contract |
|---|---|
| `action` with `name: "Auto display"` | Images in Minimal/PRO: reuse at most 1000 cached finite samples for robust editable low/high, choose Linear/Sqrt/Asinh and reset bias/contrast. Does not create PRO or modify source pixels; no effect on spectrum plots. |
| `window.FitsViewer.command(operation, params)` | Promise resolving when a command is accepted or rejected. File/science jobs finish asynchronously; read `state`/`result` afterwards. |
| `view` | Merge bounded display fields. Eight stretches: Linear, Log, Power, Sqrt, Squared, Asinh, Sinh, Histogram Equalization (`pow`/`histequ` accepted). `render.stretch_exponent` is finite, greater than 1 and at most 1,000,000; default 1000. |
| `preferences` | Replaces bounded schema-1 preferences; omitted fields take defaults. Preserve `state.preferences` when changing one field. `mode` controls the current window; `default_mode` controls new windows (both Minimal/PRO). The settings UI edits the default independently. `ribbon_tab`/`ribbon_collapsed` and the separate image/spectrum PRO widths/collapse flags preserve presentation. |
| `session.export` / `session.apply` | Export or restore schema 1, one to 32 file views, at most 64 apertures per view and 1 MiB total. Restore requires a finished current file load. |
| `GET /api/session?seq=N` | Authenticated retrieval of an already queued session from `/api/open`; the last 32 open requests are retained. Expired requests return 404. |
| `coordinates`, `coordinates.gaia`, `coordinates.cancel`, `coordinates.use_gaia` | Shared panel, asynchronous bounded Gaia solution, cancellation and accepted WCS overlay selection. Original pixels/header remain unchanged. |
| `POST /api/science` | JSON `{operation, params}`. Rust handles `measure`; optional environment Python handles science jobs. Browser input paths remain within configured roots; exports return private-cache artifact identifiers. |
| `header.select` / science `header` | UI selects an existing zero-based HDU; science parameters are `path` and `hdu`. Returns the original `header` with `merged:false`; text is loaded only when selected. |
| `aperture.uncertainty` | Configure `same_file`, optional `path`, zero-based `hdu` and explicit `kind` sigma/variance/ivar. Same-file errors require a different selected 2D HDU. Measurement results record source path/HDU/BUNIT/convention and convert errors once to sigma. No registration/reprojection is inferred. |
| `aperture.select` / `aperture.delete` | Select a zero-based `index`, or delete the current selection (optional `index`). Requires an image; spectra are rejected. Deletion preserves other apertures and supports UI Undo/Redo. |
| `aperture.resize` / `aperture.move` / `aperture.update` | Edit the selection, optionally selecting `index` first. Resize takes `radius` in pixels. Move takes a zero-based `center: [x,y]`. Update takes both and optionally `sky: [inner,outer]`. Radius is 0.1–9999.8 pixels; center is clamped to the image. Existing geometry/results for other apertures remain unchanged. Changed measurements must be recomputed. |
| `GET /api/artifact?id=...` | Authenticated download of an owned derived artifact. Page teardown releases its ownership. |
| `aperture.template` / `aperture.template_load` | PRO template paste; `preview:true` stages it. `aperture.preview.apply/cancel` confirms or discards staged apertures. Target values are measured again. |
| `pro.select_row` / `annotation.set` | PRO row focus (zero-based row) and bounded pixel/sky/canvas annotations. Scientific column names are preserved. |
| `spectral.lines` | PRO spectrum overlays only. `unit` is Angstrom/nm/um, `redshift` is finite with -1 < z <= 20, `line_width` is 0.5–5 display pixels, `tolerance` is 0.01–1000 observed Angstrom. `visible` selects reference names; `air` and `show_candidates` are booleans. Invalid updates are rejected atomically. |
| `pro.run` with `operation: "spectral_candidates"` | Starts an owned background job using current core-selected spectrum columns and the explicitly confirmed wavelength unit. Optional external Specutils/Astropy/SciPy packages come from the selected interpreter. Results are provisional emission/absorption features; changing unit/air/source invalidates incompatible late results. |
| `spectral.analysis` | PRO spectrum only. Merge `parameters` (source-coordinate region, continuum windows, masks, continuum/smoothing/profile settings) and `display` (Observed/Rest/Velocity axis, relative blackbody, preview visibility/custom references). Optional `action` continuum/measure/smooth/fit starts a cancellable owned external job. `unit`/`air`/`redshift` use the shared line context. Configuration revisions and source/columns reject incompatible late results. |
| `spectral.overlay` | PRO comparison display: zero-based `index` in the multi-file workspace and `settings` with enabled/label/epoch/scale/offset/unit/redshift/air. Up to eight enabled comparisons reuse cached Arc source arrays. Unknown wavelength units need confirmation; no automatic flux conversion, resampling or spectrum subtraction. |
| `layer.configure` | Common PRO layer metadata: zero-based `index`, `settings` with enabled/hidden, opacity 0–1, bounded order, label, scale/offset, and optional spectrum unit/z/epoch. Presentation order does not change file indices or measurement arrays. |
| `image.match_flux` | PRO image only: target layer `index` and optional external `params`. Builds the current source/reference/revision context, then runs an owned image_flux_match job. Apply only a successful finite positive factor matching both source HDUs and the current reference. |
| `science image_flux_match` | Stateless selected-environment NumPy/SciPy/Astropy field-star relative aperture-flux matching. WCS is the default; pixel mode requires explicit assume_aligned. Unknown/different filters block by default. No absolute zero point or exposure-time correction. See [image layers](IMAGE_LAYERS.md). |
| `science spectral_analysis` | CLI stateless external adapter with flat JSON parameters and action continuum/measure/smooth/fit. NumPy/SciPy are imported in the selected interpreter only when requested. See [spectral analysis](SPECTRAL_ANALYSIS.md) for complete scientific assumptions and an example. |

The read-only `state.pro_panel` reports context, rendered width and collapse state.
`state.selected_aperture` and `state.aperture_tool` expose current aperture editing state.
`state.wcs_cache` reports a document-scoped cache of at most 512 exact Astropy
pixel responses and 1 MiB. No interpolation or full-image coordinate array is
used. Document/HDU/WCS changes and close invalidate these samples. The backend
reuses up to 12 full Astropy WCS models and verifies file identity before reuse.
Gaia's accepted solution is retained in the view/session WCS override; it does
not overwrite the FITS header. `state.wcs_pixel` identifies the displayed sample,
and stale replies cannot label a different `state.cursor_pixel`.

`image_pro_width` / `spectrum_pro_width` accept 220–1200 px in preferences; the
actual viewport constrains the panel to leave room for the plot. The matching
`image_pro_collapsed` / `spectrum_pro_collapsed` flags hide the panel while PRO
jobs and line state remain alive. Switching to Minimal releases PRO resources.
The read-only `state.spectral_analysis` reports parameters, display settings,
configuration revision, derived-series sizes and per-action measurement/fit
status. Mini and image contexts have no spectral analysis object. Session views
preserve `spectrum_overlay` presentation metadata, without eagerly loading spectra.
`state.image_layer_renderer` reports cached texture count, display pixels, RGBA
bytes and build count. Auxiliary textures are capped at eight by 1 Mi pixels;
the full-resolution active original is separate. Mini releases this renderer.

Histogram equalization uses a 4096-bin image CDF shared by the image and its colorbar;
the GUI caches it until the file, stretch or display limits change. Scalar
`Stretch::apply` has no image context and falls back to linear for that choice;
use `display::ScaleMapping` or the image renderer for actual histogram equalization.
See [scientific tools and numerical limits](SCIENTIFIC_TOOLS.md).

The spectrum viewer supports linear 1D FITS coordinates and legacy DC-FLAG=1 log10 dispersion, recognized scalar or single-row fixed-vector binary tables, and UTF-8 numeric text. Other spectral WCS algorithms display sample indices explicitly. General tables, variable-length arrays and multiple spectral orders are outside this interface. `.fz` codecs are RICE_1, GZIP_1 and GZIP_2; unsupported codecs return 422.

The authenticated Jupyter endpoint GET `<base_url>/fitsviewer-settings` returns only `{"default_viewer": true|false}` for the frontend factory. It does not start the Rust backend.

HTTP errors: 403 outside read roots/traversal, 404 missing file, 422 unsupported or invalid FITS, 500 worker failure. Original inputs are not modified. Derived exports and transferred resources are written to the owned temporary cache.

See [resource lifecycle](RESOURCE_LIFECYCLE.md) for document/role generations,
bounded job history, cache references and cleanup. HTTP packets that claim an
artifact for a closed document return 409.
