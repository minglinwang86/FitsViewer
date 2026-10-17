# FitsViewer 0.1.4rc2 validation / 验证记录

Validated on 2026-10-17 with synthetic FITS images and isolated test environments. The package, import, command, Jupyter extension and desktop product use the FitsViewer name; the release installs no old aliases.

| Check / 检查 | Result / 结果 |
|---|---|
| Linux: Python 3.14.6 + JupyterLab 4.6.4 | PASS |
| Linux: Python 3.10.12 + JupyterLab 4.4.0 | PASS |
| Windows: Python 3.13.5 + JupyterLab 4.6.4 | PASS |
| JupyterLab FITS double-click, scale, right-drag brightness/contrast, CLI opening, Notebook embedding and URL prefixes | PASS on Linux and Windows; zero page JavaScript errors |
| Linux pip/source installer, selected environment with spaces, binding repair, ports and uninstall | PASS |
| Linux MIME association, previous-default restoration and native launch under Xvfb | PASS |
| Windows native rendering and close | PASS; three cycles, including a Unicode filename |
| Windows EXE icon and product metadata | PASS; embedded icon group, FitsViewer product name and fitsviewer.exe filename |
| Windows Setup: desktop only, Python/venv and Conda targets | PASS |
| Windows defaults on/off and uninstall | PASS; previous fallback defaults restored; protected UserChoice and generic .gz association preserved |
| Invalid Python target and JupyterLab 5 downgrade guard | PASS; rejected before installation; 5.x guard tested with synthetic package metadata |
| glibc 2.28 and 2.31 loaders: packaged backend, FITS API and resource discovery | PASS; isolated Debian library sets |
| Rust core numerical tests | 7 passed, 0 failed |
| Both wheel metadata checks | twine check --strict PASS |
| Compiled source comparison | Public Rust, Python, web and installer source matches the container build source, allowing line-ending normalization |

Wheel identities:

| Asset | SHA256 |
|---|---|
| fitsviewer-0.1.4rc2-py3-none-manylinux_2_28_x86_64.whl | 65de76dd9a62b990adce99176868ee00dbde7f4d05602366e87933b7c154dedb |
| fitsviewer-0.1.4rc2-py3-none-win_amd64.whl | 5cbd1b6be2f31e0be8a2e98c81af253832a0918f368f84929b0cf2de54714741 |

The release's SHA256SUMS covers the final source archive, wheels, Windows Setup and portable ZIP. Private-marker archive scans include nested packages and native binaries; public reports omit personal paths, hostnames, credentials and connection details. Upstream license attribution is retained. The README screenshot shows the running viewer with bundled synthetic data.

The earlier [multi-file memory measurements](MEMORY-0.1.4rc1.md) belong to rc1; they are not new rc2 measurements. This release reran functional integration and native render/close checks. Browser allocators can retain capacity after documents close, so closing a viewer does not promise an immediate return of all process memory to the operating system.

Validation does not cover every Linux distribution, actual X2Go forwarding, Safari, Windows ARM, Alpine/musl, macOS native binaries, or JupyterLab 5.x. The glibc loader checks are not full-distribution desktop tests. Windows binaries are unsigned. Hosted CI results are separate from these locally executed checks.

中文：本次在隔离环境中完成 Linux 最低版本与较新版本、Windows 原生窗口和 Jupyter 集成、Python/Conda 安装选择、默认查看器切换及卸载恢复。5.x 测试只验证安装前防降级保护，不代表完成 JupyterLab 5 兼容性验证。上表列出实际验证范围，旧版内存测量单独标明来源。
