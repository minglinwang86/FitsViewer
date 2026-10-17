#!/bin/bash
set -euo pipefail
source "$(dirname "$0")/env.sh"
cd "$FITSVIEW_HOME"
# Remap compiler source locations in both native and WASM release artifacts.
# 原生和 WASM 发行物均使用通用源码路径，避免暴露本机目录。
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--remap-path-prefix=$FITSVIEW_HOME=fitsview-source --remap-path-prefix=${CARGO_HOME}=cargo-source"
repair=()
portable=false
for option in "$@"; do
  case "$option" in
    --repair) repair=(--repair) ;;
    --portable) portable=true ;;
    *) printf 'Usage: bash scripts/build.sh [--portable] [--repair]\n' >&2; exit 2 ;;
  esac
done
if $portable; then
  architecture="$(uname -m)"
  case "$architecture" in x86_64|aarch64) ;; *) printf 'Unsupported portable build architecture\n' >&2; exit 2 ;; esac
  triple="$architecture-unknown-linux-gnu"
  cargo zigbuild --locked --release -p fitsview --target "$triple.2.28"
  mkdir -p target/release
  cp "target/$triple/release/fitsview" target/release/fitsview.candidate
  mv target/release/fitsview.candidate target/release/fitsview
  export FITSVIEW_WHEEL_PLATFORM="manylinux_2_28_$architecture"
else
  cargo build --locked --release -p fitsview
fi
cargo build --locked --release -p fitsview-web --target wasm32-unknown-unknown
bindgen_version=$(python - <<'PY'
import tomllib
with open('Cargo.lock','rb') as f: lock=tomllib.load(f)
print(next(p['version'] for p in lock['package'] if p['name']=='wasm-bindgen'))
PY
)
if ! command -v wasm-bindgen >/dev/null || [[ "$(wasm-bindgen --version)" != "wasm-bindgen $bindgen_version" ]]; then
  cargo install wasm-bindgen-cli --version "$bindgen_version" --locked
fi
wasm-bindgen --target web --out-dir web/pkg --no-typescript target/wasm32-unknown-unknown/release/fitsview_web.wasm
cd jupyter
npm ci
npm run build
cd "$FITSVIEW_HOME"
python scripts/build_wheel.py "${repair[@]}"
printf 'BUILD_OK\n'
