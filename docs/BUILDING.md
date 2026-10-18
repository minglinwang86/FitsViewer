# Build and validate / 构建与验证

Module plans are available without compiling: `python scripts/build_profiles.py --profile mini --plan`
or `--profile full --plan`. Presets also include `desktop-mini` and `remote-pro`; custom selections
use `--modules base,pro,jupyter`. See [architecture](ARCHITECTURE.md) and [AI guide](AI_DEVELOPMENT_GUIDE.md).
The profile driver needs explicit `--execute` to compile, and `--wheel` to package. Full acceptance is recorded in [release validation](releases/VALIDATION-0.1.5rc1.md); other presets remain developer build choices. Existing `build.sh`
is an explicit build command; `bash scripts/build.sh --plan` only prints its plan.

Runtime wheel users need Python 3.10+. Building uses Python 3.11+ with `venv`, Rust 1.99.1 and Node.js 24.22.0. The bootstrap installs development tools under `.tools/` in the checkout; it does not install FitsViewer into your science environment.

On Ubuntu 22.04, install system prerequisites in a development machine/container:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config python3-venv libxkbcommon-dev libxkbcommon-x11-dev libx11-dev libxrandr-dev libxi-dev libgl1-mesa-dev
```

Select Python 3.11+ explicitly if the system Python is older. From any checkout location:

```bash
bash scripts/bootstrap.sh /path/to/python3.11
source scripts/env.sh
bash scripts/build.sh --portable --repair
```

The bootstrap pins Rust/Node and installs `requirements-build.txt` in `.tools/build`. It prepares the WASM target and QA dependencies from `qa/package-lock.json`. It downloads tools/dependencies and therefore requires network access. Re-running it reuses matching installed tools. The build produces a Linux wheel; `--repair` checks/relabels it with auditwheel. Install the completed wheel explicitly into the desired environment.

Use `bash scripts/bootstrap.sh /path/to/python3.11 --no-qa` to omit Playwright/Chromium downloads when only building. `scripts/install.py --from-source` selects this option automatically. Browser regression tests need the QA dependencies prepared separately.

```bash
python -m twine check --strict dist/full/fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl
cargo test --locked -p fitsviewer-core
python scripts/verify_wheel.py dist/full/fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl
```

The wheel check creates temporary Python environments and its own authenticated Jupyter server, exercises default double-click opening, stretches, right-drag, CLI, Notebook embedding, binding and uninstall, then removes those environments. It writes reports and screenshots under `reports/`; existing Jupyter servers are not restarted.

To test the support floor, use a Python 3.10+ interpreter with venv and the prepared QA tools:

```bash
python scripts/verify_wheel.py dist/full/fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl --jupyterlab '==4.4.0'
```

The CI workflow builds on Ubuntu 22.04 and runs isolated wheel checks. A workflow file in the local candidate is not evidence that hosted CI has already run; consult the actual GitHub run after the source is pushed.

Optional native smoke tests need Xvfb, Pillow and X11 tools; they use a separate display and do not validate actual SSH/X2Go forwarding. Numerical checks use NumPy/Astropy and a running local viewer; `--science-file` explicitly opts into an additional local observation file. Synthetic fixtures are the default.

For multi-document memory diagnostics, install the candidate in a separate test environment and use the prepared Node QA tools:

```bash
python -m venv .tools/memory-env
.tools/memory-env/bin/python -m pip install dist/full/fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl
python scripts/verify_memory.py --python "$PWD/.tools/memory-env/bin/python" --wheel dist/full/fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl --label candidate --cycles 8
```

The test creates synthetic 2048x2048 float32 gzip FITS files; add `--uncompressed` to test ordinary `.fits` independently. It uses its own Jupyter/browser processes and records PSS/RSS, JS heap, frame counts, explicit renderer releases and weak-reference survival. `--control` exercises ordinary text editors; `--probe` inspects the loaded frontend. Browser memory-pressure notifications target only the isolated test browser. Raw process memory also includes caches and allocator reserves, so it is reported separately from live viewer/WASM objects.

Add `--lightweight-cdp` for the process-memory comparison: it controls a separate browser context without enabling Network, Debugger or Performance domains. The normal Playwright page driver remains useful for DOM/object diagnostics, but its response/debug instrumentation can materially increase retained process memory. See [the measured candidate results](releases/MEMORY-0.1.4rc1.md).

中文：运行要求 Python 3.10+，构建要求 Python 3.11+。先安装系统编译依赖，再在任意源码目录执行 bootstrap 和 build。开发工具保存在项目 `.tools/`，构建与安装分开。验证脚本只管理自己的临时环境和服务。真实原生转发、其他架构和旧 glibc 仍需独立验证。


## Next-candidate regression / 下一候选版回归

Use a dedicated dependency-ready Python environment. `verify_next.py` installs
only the selected wheel there, copies synthetic fixtures into a new directory,
and starts its own authenticated Jupyter with a URL prefix. `--keep` retains only
successfully started test services; stop the PIDs in its private browser-config.json
when finished. Do not publish that file because it contains the test token.

```bash
python scripts/verify_next.py --wheel dist/full/fitsviewer-0.1.5rc1-py3-none-manylinux_2_28_x86_64.whl --scratch /path/to/new-test-dir --native --keep
node scripts/verify_next.cjs /path/to/new-test-dir/browser-config.json
```

Linux native tests need Xvfb; `--capture-python /path/to/python-with-pillow` adds
screenshots. Windows runs the same checks in an isolated Python environment.
Recreate the reference fixtures with `scripts/make_next_fixtures.py` using Astropy
and NumPy, then run the Rust core tests. These packages are QA dependencies only.

The small vendored fitsrs codec patch is explained in
[vendor/README.fitsviewer.md](../vendor/README.fitsviewer.md). The CJK font is already
present in `web/fonts/`; regenerating it uses `scripts/prepare_cjk_font.py` and
optional fontTools. Neither fontTools nor Astropy is a runtime dependency.

中文：使用专用测试 Python 环境和全新测试目录。脚本仅启动自己的服务；原生 Linux
截图需 Xvfb/Pillow。测试令牌文件只供本地使用，不进入发行物。字体和 fitsrs 小补丁
随源码提供，用户安装预编译包不需要上述验证工具。

## Native macOS Full / macOS 原生全功能版

On an Apple Silicon or Intel Mac with Xcode Command Line Tools, Python 3.11+ and
Rust 1.99.1, download the exact Linux Full wheel named in
`config/macos-full-assets.json`. It supplies only reviewed web/Lab resources;
its Linux executable is never copied. The script verifies hashes and adapter
sources before packaging a newly compiled native Mach-O application.

```bash
python scripts/build_macos_full.py --wheel /path/to/reviewed-full.whl --execute
python -m pip install 'numpy>=2,<3' 'astropy>=6,<8'
python scripts/verify_macos_full.py --package dist/macos --output reports/macos --probe-gui
```

The output directory must be new. The workflow `.github/workflows/macos-full.yml`
runs the same build on `macos-15` and `macos-15-intel`. GUI startup evidence is
separate from interactive acceptance. The ZIP is ad-hoc signed, not notarized;
see [macOS installation](MACOS.md). This path does not create a Python wheel.

在对应处理器的 macOS 上原生编译，锁定的 Linux wheel 只提供平台无关资源。
上述 NumPy/Astropy 仅供测试，不进入应用。输出目录须全新；不会打包 Mac Python wheel，
也不修改文件关联。打包器会验证 Full 模块、资源校验值和源码一致性。
