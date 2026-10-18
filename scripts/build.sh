#!/bin/bash
# Explicit build entry. For a source-only plan, use --plan.
# 明确的构建入口；仅检查模块计划时使用 --plan，不启动编译。
set -euo pipefail
source "$(dirname "$0")/env.sh"
cd "$FITSVIEWER_HOME"
for option in "$@"; do
  if [[ "$option" == "--plan" ]]; then
    python scripts/build_profiles.py "$@"
    exit 0
  fi
done
python scripts/build_profiles.py --execute --wheel "$@"
printf 'BUILD_OK\n'
