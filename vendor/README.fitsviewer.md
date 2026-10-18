# fitsrs 0.4.1 compatibility patch

This directory contains the published `fitsrs` 0.4.1 crate source, excluding its
large upstream test data. The original crate archive SHA-256 is
`f882ef4d0334f5fc1f1f23a0a8eb51c67207a58d69e0ed17039dfdc66e8a1007a`.
The Cargo workspace uses `[patch.crates-io]` to keep this change reproducible.
Upstream project: <https://github.com/cds-astro/fitsrs>.

FitsViewer exposes the existing Rice decoder and dithering constants for its
bounded tile reader. The Rice decoder writes little-endian output explicitly,
and uses wrapping addition when reconstructing signed differences, as required
by the codec's unsigned arithmetic. No new runtime dependency is introduced.

The adapter in `crates/core/src/compressed_reader.rs` handles standard descriptor
widths, edge tiles, byte shuffling, gzip fallback and floating-point quantization.
It is checked against independent Astropy-generated reference pixels. Supported
codecs are RICE_1, GZIP_1 and GZIP_2. HCOMPRESS_1, PLIO_1 and compressed null masks
are rejected explicitly.

The original MIT / Apache-2.0 licenses remain applicable. Copies are included in
`licenses/rust/fitsrs-0.4.1/`; upstream notices in the source are preserved.

此目录保留 fitsrs 0.4.1 的发布源码及原有许可。补丁仅公开已有解码器和常量、
明确字节序并修正有符号差值的环绕加法；分块兼容处理由 FitsViewer 的小型适配层
完成，不增加运行时库。升级上游版本时应重新检查并尽量移除此补丁。
