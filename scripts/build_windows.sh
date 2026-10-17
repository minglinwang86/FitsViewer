#!/bin/bash
set -euo pipefail
source "$(dirname "$0")/env.sh"
cd "$FITSVIEWER_HOME"
command -v x86_64-w64-mingw32-gcc >/dev/null || { printf 'Install the MinGW x86_64 compiler before cross-building.\n' >&2; exit 1; }
rustup target add x86_64-pc-windows-gnu
base_flags="${RUSTFLAGS:+$RUSTFLAGS }-C strip=symbols --remap-path-prefix=$FITSVIEWER_HOME=fitsviewer-source --remap-path-prefix=$(realpath "$CARGO_HOME")=cargo-source --remap-path-prefix=$(realpath "$RUSTUP_HOME")=rust-toolchain"
RUSTFLAGS="$base_flags -C target-feature=+crt-static" cargo build --locked --release -p fitsviewer --target x86_64-pc-windows-gnu
RUSTFLAGS="$base_flags" cargo build --locked --release -p fitsviewer-web --target wasm32-unknown-unknown
wasm-bindgen --target web --out-dir web/pkg --no-typescript target/wasm32-unknown-unknown/release/fitsviewer_web.wasm
cd jupyter
npm ci
npm run build
cd "$FITSVIEWER_HOME"
python scripts/build_wheel.py --windows
printf 'WINDOWS_BUILD_OK\n'
