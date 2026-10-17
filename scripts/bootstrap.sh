#!/bin/bash
set -euo pipefail
project="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project"
interpreter="${1:-python3}"
with_qa=true
for option in "${@:2}"; do
  case "$option" in
    --no-qa) with_qa=false ;;
    *) printf 'Usage: bash scripts/bootstrap.sh [PYTHON] [--no-qa]\n' >&2; exit 2 ;;
  esac
done
"$interpreter" -c 'import sys; assert sys.version_info >= (3,11), "Building requires Python 3.11+"'
mkdir -p .tools
export CARGO_HOME="$project/.tools/cargo"
export RUSTUP_HOME="$project/.tools/rustup"
export PATH="$CARGO_HOME/bin:$project/.tools/node/bin:$PATH"
if ! command -v rustup >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o .tools/rustup-init.sh
  sh .tools/rustup-init.sh -y --profile minimal --no-modify-path --default-toolchain none
fi
rustup toolchain install 1.99.1 --profile minimal --component rustfmt,clippy --target wasm32-unknown-unknown
if [[ ! -x .tools/node/bin/node ]] || [[ "$(.tools/node/bin/node --version)" != "v24.22.0" ]]; then
  "$interpreter" - <<'PY'
from pathlib import Path
import hashlib,platform,tarfile,urllib.request
root=Path('.tools'); arch={'x86_64':'x64','aarch64':'arm64'}[platform.machine()]
name=f'node-v24.22.0-linux-{arch}.tar.xz'; base='https://nodejs.org/dist/v24.22.0/'
path=root/name; urllib.request.urlretrieve(base+name,path)
sums=urllib.request.urlopen(base+'SHASUMS256.txt').read().decode()
expected=next(line.split()[0] for line in sums.splitlines() if line.endswith(' '+name))
assert hashlib.sha256(path.read_bytes()).hexdigest()==expected, 'Node checksum mismatch'
with tarfile.open(path) as archive: archive.extractall(root,filter='data')
link=root/'node'
if link.is_symlink(): link.unlink()
elif link.exists(): raise RuntimeError('Existing .tools/node is not a symlink; select Node manually')
link.symlink_to(name.removesuffix('.tar.xz'),target_is_directory=True)
PY
fi
"$interpreter" -m venv .tools/build
.tools/build/bin/python -m pip install -r requirements-build.txt
if $with_qa; then
  npm ci --prefix qa
  ./qa/node_modules/.bin/playwright install chromium
fi
printf 'Development tools ready. Run: source scripts/env.sh\n'
