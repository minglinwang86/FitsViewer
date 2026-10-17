# Build and validate / 构建与验证

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
python -m twine check --strict dist/fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl
cargo test --locked -p fitsviewer-core
python scripts/verify_wheel.py dist/fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl
```

The wheel check creates temporary Python environments and its own authenticated Jupyter server, exercises default double-click opening, stretches, right-drag, CLI, Notebook embedding, binding and uninstall, then removes those environments. It writes reports and screenshots under `reports/`; existing Jupyter servers are not restarted.

To test the support floor, use a Python 3.10+ interpreter with venv and the prepared QA tools:

```bash
python scripts/verify_wheel.py dist/fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl --jupyterlab '==4.4.0'
```

The CI workflow builds on Ubuntu 22.04 and runs isolated wheel checks. A workflow file in the local candidate is not evidence that hosted CI has already run; consult the actual GitHub run after the source is pushed.

Optional native smoke tests need Xvfb, Pillow and X11 tools; they use a separate display and do not validate actual SSH/X2Go forwarding. Numerical checks use NumPy/Astropy and a running local viewer; `--science-file` explicitly opts into an additional local observation file. Synthetic fixtures are the default.

For multi-document memory diagnostics, install the candidate in a separate test environment and use the prepared Node QA tools:

```bash
python -m venv .tools/memory-env
.tools/memory-env/bin/python -m pip install dist/fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl
python scripts/verify_memory.py --python "$PWD/.tools/memory-env/bin/python" --wheel dist/fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl --label candidate --cycles 8
```

The test creates synthetic 2048x2048 float32 gzip FITS files; add `--uncompressed` to test ordinary `.fits` independently. It uses its own Jupyter/browser processes and records PSS/RSS, JS heap, frame counts, explicit renderer releases and weak-reference survival. `--control` exercises ordinary text editors; `--probe` inspects the loaded frontend. Browser memory-pressure notifications target only the isolated test browser. Raw process memory also includes caches and allocator reserves, so it is reported separately from live viewer/WASM objects.

Add `--lightweight-cdp` for the process-memory comparison: it controls a separate browser context without enabling Network, Debugger or Performance domains. The normal Playwright page driver remains useful for DOM/object diagnostics, but its response/debug instrumentation can materially increase retained process memory. See [the measured candidate results](releases/MEMORY-0.1.4rc1.md).

中文：运行要求 Python 3.10+，构建要求 Python 3.11+。先安装系统编译依赖，再在任意源码目录执行 bootstrap 和 build。开发工具保存在项目 `.tools/`，构建与安装分开。验证脚本只管理自己的临时环境和服务。真实原生转发、其他架构和旧 glibc 仍需独立验证。
