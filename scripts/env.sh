#!/bin/bash
# Source this only for development. / 仅开发构建时加载。
FITSVIEW_HOME="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export FITSVIEW_HOME
export CARGO_HOME="$FITSVIEW_HOME/.tools/cargo"
export RUSTUP_HOME="$FITSVIEW_HOME/.tools/rustup"
export PATH="$FITSVIEW_HOME/.tools/build/bin:$CARGO_HOME/bin:$FITSVIEW_HOME/.tools/node/bin:$FITSVIEW_HOME/.tools/bin:$PATH"
