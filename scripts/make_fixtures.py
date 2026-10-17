"""Small, reproducible FITS cases; all files are synthetic. / 小型可复现合成样本。"""
from pathlib import Path
import gzip
import json
import numpy as np
from astropy.io import fits

root = Path(__file__).resolve().parents[1]
dest = root / "examples"
dest.mkdir(exist_ok=True)
rng = np.random.default_rng(20261016)
y, x = np.mgrid[:512, :512]
data = rng.normal(100.0, 4.0, (512, 512))
for cx, cy, amplitude, sigma in [(110, 160, 1300, 2.8), (315, 235, 4200, 3.8), (390, 380, 1900, 2.1), (225, 405, 650, 2.4), (160, 300, 850, 2.5), (370, 110, 2100, 2.9), (255, 150, 400, 2.2)]:
    data += amplitude * np.exp(-((x-cx)**2 + (y-cy)**2)/(2*sigma*sigma))
data[0, 0], data[0, -1], data[-1, 0], data[-1, -1] = 11, 22, 33, 44
data[20, 20] = np.nan
hdu = fits.PrimaryHDU(data.astype("float32"))
hdu.header["OBJECT"] = "Synthetic star field; not a science observation"
hdu.writeto(dest / "starfield.fits", overwrite=True)
with gzip.open(dest / "starfield.fits.gz", "wb") as out:
    out.write((dest / "starfield.fits").read_bytes())

scaled = fits.PrimaryHDU(np.array([[-32768, -2, 0], [4, 10, 32767]], dtype="int16"))
scaled.header["BSCALE"] = 2.0
scaled.header["BZERO"] = 100.0
scaled.header["BLANK"] = -32768
scaled.writeto(dest / "scaled_int16.fits", overwrite=True)
fits.PrimaryHDU(np.array([[2**60+1, -(2**60+1)], [0, 42]], dtype="int64")).writeto(dest / "exact_int64.fits", overwrite=True)
double = fits.PrimaryHDU(np.array([[1.000000000000001, np.nan], [-2.0, 1e100]], dtype="float64"))
double.header["BSCALE"] = 0.5
double.header["BZERO"] = -10.0
double.writeto(dest / "scaled_float64.fits", overwrite=True)
fits.PrimaryHDU(np.full((8, 8), np.nan, dtype="float32")).writeto(dest / "all_nan.fits", overwrite=True)
fits.PrimaryHDU(np.full((8, 8), 5.0, dtype="float32")).writeto(dest / "constant.fits", overwrite=True)
fits.PrimaryHDU(np.arange(60, dtype="float32").reshape(3, 4, 5)).writeto(dest / "cube_unsupported.fits", overwrite=True)
fits.PrimaryHDU(data.astype("float32")-100).writeto(dest / "negative_difference.fits", overwrite=True)
fits.PrimaryHDU(np.arange(12, dtype="uint8").reshape(3, 4)).writeto(dest / "uint8.fits", overwrite=True)
fits.PrimaryHDU(np.arange(-6, 6, dtype="int32").reshape(3, 4)).writeto(dest / "int32.fits", overwrite=True)
fits.HDUList([fits.PrimaryHDU(), fits.ImageHDU(np.arange(20, dtype="float32").reshape(4, 5), name="SCI"), fits.ImageHDU(np.ones((4, 5), dtype="float32")*2, name="ERR")]).writeto(dest / "multi_hdu.fits", overwrite=True)
(dest / "空 格 image.FITS").write_bytes((dest / "starfield.fits").read_bytes())
raw = (dest / "starfield.fits").read_bytes()
(dest / "truncated.fits").write_bytes(raw[:2880+512*512*4-4])
(dest / "not_fits.fits").write_text("not a FITS file")
notebook = {
    "nbformat": 4, "nbformat_minor": 5,
    "metadata": {"kernelspec": {"display_name": "Python 3 (ipykernel)", "language": "python", "name": "python3"}},
    "cells": [
        {"cell_type": "markdown", "id": "description", "metadata": {}, "source": ["# FITSView notebook example\n", "Synthetic data. The viewer supports wheel zoom, drag pan, and independent limits/stretch controls."]},
        {"cell_type": "code", "id": "viewer", "metadata": {}, "execution_count": None, "outputs": [], "source": ["from fitsview_jupyter import show\n", "show('fitsview/examples/starfield.fits', height=650)"]},
    ],
}
(dest / "Notebook_demo.ipynb").write_text(json.dumps(notebook, ensure_ascii=False, indent=2))
print(json.dumps({"seed": 20261016, "synthetic": True, "files": sorted(p.name for p in dest.iterdir())}, ensure_ascii=False))
