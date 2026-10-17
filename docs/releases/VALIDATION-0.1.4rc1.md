# Candidate validation / 候选版验证

Validation for `0.1.4rc1`, prepared 2026-10-17 using synthetic data and isolated test environments.

| Check | Result |
|---|---|
| Python 3.14.6 + JupyterLab 4.6.4 | PASS |
| Python 3.10.12 + JupyterLab 4.4.0 | PASS |
| FITS double-click, stretch, right-drag, CLI, Notebook and URL prefix | PASS; zero page JS errors |
| Interactive Python path with spaces; explicit default settings | PASS |
| Linux MIME association, native GIO launch, previous-default restoration | PASS; isolated Xvfb |
| Install without Rust/Node in target PATH; self-repack; binding repair; uninstall | PASS |
| Ports, busy-server preservation, SSH command and Conda target inspection | PASS |
| Source build and selected-environment installer | PASS; build and install workflow |
| Rust core | 7 passed, 0 failed |
| Wheel metadata | twine check --strict PASS |
| glibc 2.28 / 2.31 loaders and FITS backend API | PASS; isolated Debian libraries |
| requirements.txt matches package metadata and is bundled | PASS |
| Full dependency/font legal files, install and installed-package self-repack | PASS; 509 legal files preserved byte for byte |
| Multi-file viewer/WASM teardown | PASS; [measured scope and caveats](MEMORY-0.1.4rc1.md) |

Wheel: `fitsview_jupyter-0.1.4rc1-py3-none-manylinux_2_28_x86_64.whl`. SHA256: `0e0083fec5e858cf9e7c5f0c93f6fb04ddbfd5aabd42f1ec055fdf69de858978`.

Conda target resolution was inspected; full isolated installations use Python/venv targets. A real X2Go/Safari session, ARM/Alpine wheels, maximum-size image memory tests and hosted CI remain unvalidated. Loader checks are not full-distribution tests. Public summaries omit local installation paths, process identifiers and connection details.

Release builds discover their resources at runtime and remap compiler source paths. Archives are checked for private markers before publication. Dependency authors' attribution is retained.

中文：验证使用合成图像和隔离环境；公开报告只保留通用测试方法与结果。各项验证边界以上表和说明为准。

The privacy rebuild passed the complete functional matrix again, plus native-window, glibc/resource-discovery, core, legal-transfer and independent FITS/gzip memory checks. Compiler source paths were remapped; the installed server locates resources without an embedded build directory.
