"""Regression for Git EOL normalization without accepting stale science source.

行尾变化应通过；真实源码变化仍须拒绝。只检查资源，不编译程序。
"""
from pathlib import Path
import argparse
import json
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import build_macos_full as mac


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", type=Path, required=True)
    args = parser.parse_args()
    wheel = args.wheel.resolve()
    mac.validate_wheel(wheel, mac.lock())
    parent = ROOT / "build/macos-packaging-tests"
    parent.mkdir(parents=True, exist_ok=True)
    original_root = mac.ROOT
    with tempfile.TemporaryDirectory(prefix="owned-", dir=parent) as temporary:
        work = Path(temporary)
        assert work.resolve().is_relative_to(parent.resolve())
        simulated = work / "checkout"
        adapters = simulated / "web/python"
        adapters.mkdir(parents=True)
        for source in (ROOT / "web/python").glob("*.py"):
            # Reproduce Linux/macOS checkout bytes rather than Windows autocrlf.
            blob = subprocess.check_output(["git", "show", "HEAD:web/python/" + source.name], cwd=ROOT)
            (adapters / source.name).write_bytes(blob.replace(b"\r\n", b"\n"))
        mac.ROOT = simulated
        try:
            mac.extract_resources(wheel, work / "accepted")
            changed = adapters / "fitsviewer_science.py"
            current = changed.read_bytes()
            assert b"MAX_INPUT=512*1024*1024" in current
            changed.write_bytes(current.replace(b"MAX_INPUT=512*1024*1024", b"MAX_INPUT=512*1024*1024-1", 1))
            try:
                mac.extract_resources(wheel, work / "rejected")
            except ValueError as error:
                assert str(error) == "Shared scientific adapter is stale: fitsviewer_science.py"
            else:
                raise AssertionError("A non-line-ending source change was accepted")
        finally:
            mac.ROOT = original_root
    print(json.dumps({"passed": True, "git_lf_vs_wheel_crlf_accepted": True,
                      "changed_scientific_limit_rejected": True}))


if __name__ == "__main__":
    main()
