#!/bin/bash
# Source this only for development. / 仅开发构建时加载。
FITSVIEWER_HOME="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export FITSVIEWER_HOME
export CARGO_HOME="$FITSVIEWER_HOME/.tools/cargo"
export RUSTUP_HOME="$FITSVIEWER_HOME/.tools/rustup"
export PATH="$FITSVIEWER_HOME/.tools/build/bin:$CARGO_HOME/bin:$FITSVIEWER_HOME/.tools/node/bin:$FITSVIEWER_HOME/.tools/bin:$PATH"
