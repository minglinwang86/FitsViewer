# Scope and remaining work / 当前范围与后续工作

The 0.1.6 Full release implements the former Chinese-path, stable status panel,
tiled FITS and basic spectrum tasks, plus Minimal/PRO science workspaces.
See [release notes](releases/v0.1.6.md), [validation](releases/VALIDATION-0.1.6.md)
and the illustrated [English](../README.md) / [Chinese](../README.zh-CN.md) guides.

0.1.6 已实现中文路径、稳定信息栏、分块压缩 FITS、基础光谱，以及极简／PRO
科学工具。功能与实际测试范围见上述说明；早期调查记录保留在开发工作日志中。

Still outside this release / 本版尚未包含：

- HCOMPRESS/PLIO and compressed null-mask columns / 相应压缩编码与空值列。
- Cube slicing, general FITS-table editing, variable-length/multiple-order spectra,
  IRAF multispec and full nonlinear spectral WCS / 数据立方、通用表格与完整复杂光谱格式。
- Spectrum-to-spectrum subtraction, coaddition and physical flux-unit conversion /
  多谱相减、合并与物理流量单位换算。
- PSF photometry, absolute photometric zeropoints, flux-conserving reprojection and
  all-sky blind astrometry / PSF 测光、绝对零点、通量守恒重投影与全天盲解。
- JupyterLab 5, Linux/Windows ARM and Linux musl packages / 相应环境兼容与安装包。
- A notarized macOS installer, local Mac Jupyter wheel and complete Mac GUI acceptance /
  公证 Mac 安装器、本机 Jupyter wheel 与完整交互验收；当前提供两种架构的原生 portable ZIP。

Future work should retain the shared module registry, external science dependencies,
explicit uncertainty conventions and bounded document/worker/artifact ownership.
后续实现须继续保持模块边界、外部科学环境、明确误差约定与有界资源生命周期。
