# FitsViewer

[中文说明](README.zh-CN.md) · [Commands](docs/COMMANDS.md) · [Build](docs/BUILDING.md) · [Releases](https://github.com/minglinwang86/FitsViewer/releases)

FitsViewer is a lightweight FITS viewer for quickly opening and visually inspecting research images on remote servers, in local containers, or on desktop computers. It supports FITS files and gzip-compressed FITS such as `.fits.gz`.

Double-click files in JupyterLab, launch the viewer from a Linux terminal, embed it in a Notebook, or use a native graphical window on Linux and Windows.

## Interface preview

![FitsViewer displaying the JWST NIRCam F277W FITS image of gravitational-lensing cluster SMACS 0723](docs/images/jwst-smacs0723-interface.png)

Actual FitsViewer screenshot of the SMACS 0723 gravitational-lensing field in [Webb's First Deep Field](https://science.nasa.gov/asset/webb/webbs-first-deep-field-nircam-image/). This grayscale view uses a 1.4 arcminute crop of the NIRCam F277W FITS mosaic with Asinh stretch. Use the mouse wheel to zoom, left-drag to pan, and right-drag to adjust brightness/contrast.

Observations: NASA, ESA, CSA, STScI. FITS mosaic: [Gabriel Brammer (2022)](https://doi.org/10.5281/zenodo.6874301), [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). [Preview data and capture details](docs/images/README.md).

## Requirements

| Component | Requirement |
|---|---|
| Server / wheel | Linux x86_64, glibc 2.28 or newer |
| Windows desktop / wheel | Windows 10/11 x86_64, usable OpenGL; [Windows guide](docs/WINDOWS.md) |
| Python | 3.10 or newer for Jupyter integration; optional for the Windows desktop |
| JupyterLab | 4.4 or newer, below 5; install into the environment running the server |
| Browser | WebAssembly and WebGL; Linux Chromium is tested |
| Native window | Optional X11/OpenGL display, including a separately configured forwarding session |

The JupyterLab range means `4.4 <= version < 5`. JupyterLab 5.x has not been validated and is currently excluded by the Python and frontend dependency ranges. A major-version update needs an extension compatibility test before this bound can change. The native desktop window operates independently of JupyterLab.

Windows and macOS can also use a browser connected to the Linux server through SSH. They do not need the Linux wheel installed locally. Safari and actual X2Go/X11 forwarding have not been validated. Wheel users do not need Rust, Node.js or `jupyter lab build`.

See [DEPENDENCIES.txt](DEPENDENCIES.txt) for the full environment checklist, [requirements.txt](requirements.txt) for pip runtime dependencies, and [requirements-build.txt](requirements-build.txt) for pinned build tools.

The prebuilt wheel requires GNU/Linux with glibc; it does not cover Alpine/musl or ARM. The source build selects the host's x86_64/aarch64 target, but ARM has not been validated. A manylinux tag specifies a library compatibility baseline, not certification of every distribution or desktop.

## Windows desktop installation

Windows builds include a Setup installer, a portable ZIP and a Python wheel. Published files are listed in [GitHub Releases](https://github.com/minglinwang86/FitsViewer/releases); source build instructions are in the [Windows guide](docs/WINDOWS.md). Setup offers an optional existing Python/Conda environment for JupyterLab/Notebook and a separate default FITS file association checkbox. The desktop viewer needs no Python or server port.

## Linux: install and double-click a FITS file

Use a Linux wheel downloaded from [GitHub Releases](https://github.com/minglinwang86/FitsViewer/releases) or built from source. The Python package and command are `fitsviewer`; this project is not yet published on PyPI. Place the matching wheel in the current directory, activate the Conda or virtual environment that runs JupyterLab, then install it:

```bash
python -m pip install --upgrade ./fitsviewer-*-py3-none-manylinux_2_28_x86_64.whl
fitsviewer jupyter start --root "$HOME/data" --port 8082
```

The data directory must already exist. Open the printed login URL. In JupyterLab's file browser, double-click a `.fits`, `.fit`, `.fts`, or corresponding gzip file. FitsViewer registers as the default viewer. Restart an already running JupyterLab server after installation, then refresh the browser.

Verify registration:

```bash
jupyter server extension list
jupyter labextension list
fitsviewer status
```

The server extensions `fitsviewer` and `jupyter_server_proxy`, and the Lab extension `fitsviewer`, should be enabled. If another default was chosen, use **Open With → FitsViewer** or update JupyterLab's default viewer setting. Binding commands are in [the command guide](docs/COMMANDS.md).

To copy a complete installation into another environment:

```bash
fitsviewer install --python /path/to/env/bin/python --check
fitsviewer install --python /path/to/env/bin/python
```

Alternatively select `--prefix /path/to/env` or `--conda-env astro`. Add `--with-jupyter` if the target needs JupyterLab installed or upgraded. Success output is in English and reports the target environment, ports, binding status and binding commands. Direct `pip install` uses pip's standard output.

### Environment and default-viewer choices

Running `fitsviewer install` without a target in a terminal asks for the current Python, another Python path, or a Conda environment, then asks about Jupyter and desktop defaults. For unattended SSH installation, pass the choices explicitly:

```bash
fitsviewer install --conda-env astro --default-viewer yes --desktop-default no
fitsviewer install --python /path/to/env/bin/python --default-viewer no --desktop-default no
```

For a first pip installation into an existing Conda environment:

```bash
conda run -n astro python -m pip install ./fitsviewer-*-py3-none-manylinux_2_28_x86_64.whl
conda run -n astro fitsviewer defaults set --jupyter yes --desktop no
```

For a Python/venv target, run its `bin/python -m pip install ...` and then its `bin/fitsviewer defaults set ...`. Replace `yes` with `no` to keep FitsViewer available through Open With without making it JupyterLab's default. No desktop is required for this route.

For Git source installation:

```bash
git clone https://github.com/minglinwang86/FitsViewer.git
cd FitsViewer
python3 scripts/install.py --from-source --build-python python3.11 --conda-env astro --default-viewer yes --desktop-default no
```

The same source installer accepts `--python` or `--prefix`, and `--wheel ./file.whl` to install an existing wheel without building Rust/Node. See [BUILDING.md](docs/BUILDING.md) for build prerequisites. Add `--with-jupyter` if the selected runtime environment needs JupyterLab.

## Remote Linux through SSH

Run on the Linux server:

```bash
fitsviewer jupyter start --root "$HOME/data" --port 8082 --browser-port 18082
fitsviewer tunnel --ssh-host user@server.example.com --ssh-port 22 --local-port 18082 --remote-port 8082
```

The second command prints an SSH command. Run it on the computer with your browser and keep its terminal open. For the example above:

```bash
ssh -N -o ExitOnForwardFailure=yes -o ServerAliveInterval=30 -L 127.0.0.1:18082:127.0.0.1:8082 -p 22 user@server.example.com
```

Use the printed Jupyter login URL with local browser port `18082`. Replace the example user/host with your reachable SSH endpoint. The Rust proxy receives an automatic internal port and does not need a separate forward. Login URLs contain an authentication token; keep them private.

## Terminal and Notebook

```bash
fitsviewer /path/to/image.fits
fitsviewer /path/to/image.fits --hdu 1
fitsviewer /path/to/image.fits --no-browser
fitsviewer --native /path/to/image.fits
fitsviewer ports
fitsviewer ports set --jupyter 8084 --browser 18084
```

The ordinary command uses the linked Jupyter server in the same environment, or starts a standalone browser viewer if none is available. **Open in window** creates a popup after a user click; later terminal requests can switch its file. Native mode needs an existing display connection. Port changes apply to future starts and do not alter running SSH tunnels.

### Double-click on a Linux desktop or X2Go session

```bash
fitsviewer desktop install
fitsviewer defaults status
fitsviewer defaults set --jupyter yes --desktop no
```

`desktop install` registers a user-level file association and native launcher for FITS/FITS.gz. It requires `xdg-utils`, `shared-mime-info`, X11/XWayland and usable OpenGL; desktop-file-utils can refresh the launcher cache. X2Go needs the same display/GLX capability. Keep the Python/Conda environment used for registration. Desktop preferences are per user; Jupyter preferences are per environment.

`--no-default` registers only an Open With entry. `defaults set --desktop no` restores the previously recorded default; when none existed, selection is left to the desktop. Run `fitsviewer desktop remove` before uninstalling the package/environment. Refresh JupyterLab after `defaults set --jupyter yes|no`.

In a Notebook:

```python
from fitsviewer import show
show("image.fits", height=650)
```

Paths are relative to the Jupyter server's file root. If the kernel and server use different environments, install the package in the server environment for double-click opening and in the kernel environment for `show()`. `show(..., base_url="/user/name/")` can specify a URL prefix when the kernel does not inherit it.

## Controls

| Action | Control |
|---|---|
| Zoom / pan | Wheel or `+` / `-`; left-button drag |
| Brightness | Right-button horizontal drag; right is brighter |
| Contrast | Right-button vertical drag; up increases contrast |
| Reset brightness/contrast | **Reset B/C** or `R` |
| Fit / original resolution | **Fit** or `F`; **1:1** |
| Stretch | Linear, Log, Sqrt, Asinh; keys `1`–`4` |
| Limits | ZScale, 99.5%, Min/Max, Manual; edit **Low** / **High** |
| Invert / HDU / header | **Invert** or `I`; **HDU** menu; **Header** |
| Pixel readout | Hover for one-based FITS coordinates, physical value and exact raw storage value |

Display changes reuse loaded pixels and do not modify FITS files. Brightness/contrast follows stretch, independently of Low/High. ZScale uses an IRAF-style sampled fit; exact DS9 numerical equivalence is not claimed.

## Data support and limits

- Ordinary 2D Primary/Image HDUs, multiple HDUs and externally gzip-compressed FITS.
- Direct compressed-file viewing supports `.fits.gz`, `.fit.gz` and `.fts.gz`. Extract ZIP or TAR archives before opening their FITS files.
- BITPIX 8, 16, 32, 64, -32 and -64; typed raw storage is retained.
- `BSCALE`, `BZERO`, integer `BLANK` and non-finite floating values are handled. Physical/display calculations use f64; raw large integers remain exact in the raw readout.
- Up to 512 MiB per input/decompressed file and 32 Mi pixels per image. These are capacity limits, not a performance guarantee.

Tile-compressed `.fz`, cube slicing, FITS tables, WCS sky coordinates, regions and photometry are not implemented. There is no tiled rendering, image pyramid or browser worker yet. Unsupported data produces an error or a disabled HDU. All fixtures in `examples/` are synthetic, including deliberately invalid validation inputs.

`.fits.gz` is decompressed by the Rust reader; browser memory depends on the uncompressed pixels, WASM buffers and display textures. Closing a FITS document explicitly destroys its renderer before detaching the iframe. Independent ordinary FITS and gzip FITS tests each covered six 2048x2048 float32 images over eight cycles, repeated and early closes, with no surviving WASM memory instances. An additional instrumented run found no surviving viewer widgets or iframes. Browser/Jupyter caches and allocator reserves can keep process memory above its initial level. See [the memory validation report](docs/releases/MEMORY-0.1.4rc1.md); maximum-size images and long-running sessions remain outside these tests.

## Build and contribution

[BUILDING.md](docs/BUILDING.md) provides a setup independent of any particular container path. Dependencies use Cargo/npm lockfiles, pinned build tools and a separate QA lockfile. Building produces artifacts; installation is explicit.

See [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md) and [public API](docs/API.md).

## License

Project source: [MIT](LICENSE), copyright FitsViewer contributors. Redistributed dependencies retain their own terms; [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) and `licenses/` accompany the wheel and source release. Python license metadata describes the combined distribution, including bundled fonts.
