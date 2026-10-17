# Public API contracts

The viewer reads ordinary 2D FITS images. It keeps typed storage values and separates FITS numeric scaling from display transforms.

| API | Contract |
|---|---|
| MAX_PIXELS / MAX_FILE_BYTES | 32 Mi pixels and 512 MiB; gzip output is capped during decompression. |
| HduInfo / ImageInfo | HDU index/name/type/axes and image dimensions, BITPIX, BSCALE, BZERO, BLANK, header text. |
| RawPixels | U8, I16, I32, I64, F32, F64 owned arrays. |
| RawPixels::len / is_empty / raw_string | Storage length, emptiness and exact textual storage value. raw_string requires an in-bounds index. |
| FitsImage::physical | Apply BSCALE/BZERO once; None for BLANK, non-finite input or overflow. Requires an in-bounds index. |
| FitsImage::to_packet / from_packet | Length-prefixed JSON metadata plus little-endian typed pixels. Reject invalid lengths/types/dimensions. Preserve raw i64/f64 and NaNs. |
| load_file | Native-only fitsrs reader, optional zero-based HDU. None selects the first ordinary 2D image. Errors include IO, malformed header, unsupported HDU, capacity limit and truncation. |
| Limits::ALL / name | Available interval modes and their UI labels. |
| Stretch::ALL / name / apply | Available stretches, labels and mapping of a finite normalized value clipped to [0,1]. |
| Statistics::new / bounds | Scan finite physical values; sorted sample capped at 1000. Bounds are valid for constant/all-invalid images. Manual initially uses min/max. |
| zscale | Sorted finite sample, contrast; IRAF-style iterative line fit. Empty input falls back to [0,1]. DS9 numerical identity is not guaranteed. |
| DisplayTone / DisplayTone::default / apply | Display-only bias/contrast; defaults 0.5/1. For finite level/bias and positive finite contrast, apply maps `(level-bias)*contrast+0.5` clipped to [0,1]. The UI bounds bias to [0,1] and contrast to [0.05,20]. |
| render_rgba / render_rgba_with_tone | Finite Low < High, stretch and inversion; RGBA8 in screen row order. render_rgba uses the default DisplayTone; render_rgba_with_tone applies the supplied tone after stretch and before inversion. Invalid pixels stay purple. FITS y is flipped for display. Neither function modifies the image or limits. |
| FitsApp::new / from_image | Initialize an HTTP/path-backed viewer, or a viewer with an already loaded FitsImage. |
| start / stop | WASM entry: canvas, API base URL, file path, optional token and HDU. Returns a JS error if graphics startup fails. stop destroys the managed WebRunner and its app, textures, frame callback and event subscriptions. Document teardown calls it before detaching the iframe; the page also stops polling and loses its WebGL context. |
| setup_server | Python configuration for the supervised Rust service in Jupyter Server Proxy. |
| fitsviewer console entry | Run the wheel's bundled ELF program with packaged web assets; use writable state scoped to the Python environment. Warm the same environment's Jupyter service when available. |
| CLI management | status/ports inspect live services without starting them; ports set saves per-environment future-start settings; tunnel prints a client-side SSH command. |
| Jupyter CLI | jupyter start reuses a matching linked server or starts a separate one; explicit busy ports fail without stopping another server. jupyter url prints the login token only on request. |
| Environment installer | install selects exactly one Python/prefix/Conda target, preserves venv Python symlinks, validates Python/Jupyter compatibility, and runs target pip. A terminal without a target offers interactive selection. --check is read-only. --default-viewer yes/no selects the Jupyter default and --desktop-default yes/no changes the current user's desktop association. Complete installed resources can reconstruct a platform wheel without the original download. Success output reports the target environment, ports, binding, defaults, and shell-quoted binding commands. Target inspection excludes caller FitsViewer/Python development overrides. |
| Default-viewer commands | defaults status inspects preferences; defaults set --jupyter yes/no saves the per-environment factory preference. Refresh JupyterLab to apply it; Open With remains registered. --desktop yes registers the native launcher, and no restores recorded previous defaults. |
| Desktop commands | desktop install registers per-user FITS MIME types and a launcher using the selected environment's absolute fitsviewer path; --no-default keeps an Open With entry. desktop status inspects; desktop remove restores previous defaults and removes owned files. Requires xdg-utils/shared-mime-info. |
| Jupyter server extension | Automatically bind the proxy command to ServerApp.root_dir and base_url. Record only the PID/runtime directory for CLI discovery; authentication stays in Jupyter's own runtime file. |
| show | Python show(path, hdu=None, height=650, base_url=None) returns IPython IFrame; invalid height/HDU raises ValueError. |

HTTP: GET /api/health; GET /api/image?path=...&hdu=...; POST /api/open with JSON path/hdu; GET /api/requests?after=... . Standalone APIs require the runtime token. Jupyter mode uses authenticated same-origin proxy access.

The authenticated Jupyter endpoint GET `<base_url>/fitsviewer-settings` returns only `{"default_viewer": true|false}` for the frontend factory. It does not start the Rust backend.

HTTP errors: 403 outside read roots/traversal, 404 missing file, 422 unsupported or invalid FITS, 500 worker failure. No endpoint writes science files.
