"""Compare the Rust HTTP payload with independent Astropy/NumPy reads."""
from pathlib import Path
import argparse
import json
import time
import urllib.request
import urllib.parse
import urllib.error
import numpy as np
from astropy.io import fits

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--science-file", action="append", type=Path, default=[], help="Explicit extra local input; not included in release fixtures")
args = parser.parse_args()
(root / "reports").mkdir(exist_ok=True)
runtime = json.loads((root / ".runtime/server.json").read_text())
base = f"http://127.0.0.1:{runtime['port']}"
headers = {"X-Fitsviewer-Token": runtime["token"]}
results = []


def read(path, hdu=None):
    query = {"path": str(path)}
    if hdu is not None:
        query["hdu"] = hdu
    started = time.perf_counter()
    with urllib.request.urlopen(urllib.request.Request(base + "/api/image?" + urllib.parse.urlencode(query), headers=headers)) as response:
        packet = response.read()
    count = int.from_bytes(packet[:4], "little")
    info = json.loads(packet[4:4+count])
    dtype = {8: "u1", 16: "<i2", 32: "<i4", 64: "<i8", -32: "<f4", -64: "<f8"}[info["bitpix"]]
    raw = np.frombuffer(packet[4+count:], dtype=dtype).reshape(info["height"], info["width"])
    return info, raw, (time.perf_counter()-started)*1000


cases = [(p, None) for p in sorted((root / "examples").glob("*")) if p.name.endswith((".fits", ".fits.gz", ".FITS")) and p.name not in {"not_fits.fits", "truncated.fits", "cube_unsupported.fits"}]
cases += [(root / "examples/multi_hdu.fits", 2)]
# Read a few existing science products without changing their contents.
# 只读少量现有科学数据；不修改原始文件。
for path in args.science_file:
    if path.is_file():
        cases.append((path, None))
for path, selected in cases:
    info, actual, elapsed = read(path, selected)
    with fits.open(path, do_not_scale_image_data=True, uint=False) as hdus:
        hdu = hdus[info["hdu"]]
        np.testing.assert_array_equal(actual, hdu.data)
        assert info["bscale"] == hdu.header.get("BSCALE", 1)
        assert info["bzero"] == hdu.header.get("BZERO", 0)
        assert info["blank"] == hdu.header.get("BLANK")
        physical = actual.astype("float64") * info["bscale"] + info["bzero"]
        if info["blank"] is not None:
            physical[actual == info["blank"]] = np.nan
    with fits.open(path, uint=False) as hdus:
        np.testing.assert_allclose(physical, hdus[info["hdu"]].data.astype("float64"), rtol=2e-7, equal_nan=True)
    results.append({"file": str(path), "hdu": info["hdu"], "shape": list(actual.shape), "bitpix": info["bitpix"], "raw_pixels_equal": True, "physical_values_equal": True, "elapsed_ms": round(elapsed, 2)})

negative = []
for path, expected in [(root / "examples/truncated.fits", 422), (root / "examples/not_fits.fits", 422), (root / "examples/cube_unsupported.fits", 422), (Path("/etc/passwd"), 403), (Path("../etc/passwd"), 403)]:
    try:
        read(path)
        raise AssertionError("Invalid input was accepted: " + str(path))
    except urllib.error.HTTPError as error:
        assert error.code == expected, (str(path), error.code)
        negative.append({"file": str(path), "status": error.code, "message": error.read().decode()[:180]})

report = {"passed": True, "backend": "fitsrs 0.4.1", "comparisons": results, "rejections": negative}
(root / "reports/numerical_validation.json").write_text(json.dumps(report, indent=2, ensure_ascii=False))
print(json.dumps({"passed": True, "image_comparisons": len(results), "negative_cases": len(negative)}))
