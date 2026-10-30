"""Return 0 only when the recorded platform check's inputs remain unchanged.

Return 1 to request the original test, or 2 for an invalid/missing record.
This does not replace current package integrity, render or histogram checks.
"""
from pathlib import Path
import hashlib
import json
import re
import sys


def main():
    root = Path(__file__).resolve().parents[1]
    try:
        record = json.loads((root / "docs/releases/PLATFORM-0.1.7-REUSE.json").read_text())
        assert record["schema"] == 1
        check = record["checks"][sys.argv[1]]
        assert check["accepted"] is True and check["files_sha256_lf"]
        changed = []
        for name, expected in check["files_sha256_lf"].items():
            path = (root / name).resolve()
            assert path.is_relative_to(root) and re.fullmatch(r"[0-9a-f]{64}", expected)
            if not path.is_file() or hashlib.sha256(path.read_bytes().replace(b"\r\n", b"\n")).hexdigest() != expected:
                changed.append(name)
        if changed:
            print("Run the original check; changed inputs: " + ", ".join(changed))
            return 1
        print("Reuse accepted " + sys.argv[1] + " from " + check.get("run_url", record["run_url"]))
        return 0
    except (AssertionError, KeyError, OSError, ValueError, IndexError) as error:
        print("Invalid reuse record: " + str(error), file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
