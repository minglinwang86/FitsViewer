# Changelog / 版本记录

## 0.1.4rc2 — Linux / Windows candidate, 2026-10-17

- Product branding is FitsViewer; Python distribution, import and terminal command are all `fitsviewer`, with no legacy aliases.
- Windows EXE icon/version resources and matching taskbar/shortcut identity.
- Actual interface preview in both public READMEs using a bundled synthetic image.

- Windows native desktop, Browse/Ctrl+O and drag-and-drop opening, bundled icon and explicit image close.
- Optional Python/Conda environment and independent FITS association choices in a per-user installer.
- Windows wheel, environment discovery, process checks and Jupyter command portability.
- Previous Windows fallback defaults restored on removal; protected UserChoice is handled through Windows Settings.
- Clarified the JupyterLab `>=4.4,<5` support range and its independence from native viewing.

## 0.1.4rc1 — Linux prerelease, 2026-10-17

- English/Chinese public READMEs, portable examples, support boundaries and release notes.
- Primary MIT license and bundled dependency/font notices included in distributions.
- Python long description and prerelease metadata; strict wheel metadata validation.
- Portable build/bootstrap, explicit installation, locked QA dependencies and CI checks.
- Clean source archive, checksums and a release plan awaiting maintainer approval.
- Frontend shared dependency ranges aligned with the JupyterLab 4.4 support floor.
- Portable x86_64 wheel build with a glibc 2.28 baseline.
- Terminal installation choices for Python/Conda and Jupyter/desktop defaults, shared by pip and Git routes.
- User-level Linux FITS MIME association and native desktop launcher, with commands to restore previous defaults.
- Explicit WebRunner/texture cleanup before a Jupyter document is detached; disconnected path signals and stopped popup polling on close.
- Multi-file memory diagnostics with a plain-text Jupyter control, weak-reference checks and isolated process measurements.

中文：补齐中英文说明、许可、包元数据、构建与验证入口；Linux 候选版已发布。

## 0.1.3 — local build, 2026-10-16

- English target-environment installation summary: ports, JupyterLab binding and repair commands.
- Verified enabled/disabled binding states and paths containing spaces.

## 0.1.2 — local build, 2026-10-16

- Port/status inspection, saved settings, SSH command generation and managed Jupyter startup.
- Installation into Python/prefix/Conda targets, including self-repacked wheels.

## 0.1.1 and initial MVP — local builds, 2026-10-16

- fitsrs backend, shared native/WASM viewer, JupyterLab default opening and Notebook embedding.
- Zoom/pan, display stretches/limits and right-drag brightness/contrast.

Historical entries describe local artifacts, not earlier public releases.
