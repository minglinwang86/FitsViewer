"""Synthetic reference cases for the unpublished next version.
下一版本的合成验收样本，参考值由 Astropy 解码生成。
"""
from pathlib import Path
import gzip
import json
import numpy as np
from astropy.io import fits

root = Path(__file__).resolve().parents[1]
dest = root / "examples/next"
dest.mkdir(parents=True, exist_ok=True)
cases = []
data = (np.arange(16 * 24).reshape(16, 24) * 7 - 600).astype("int16")
for algorithm in ["RICE_1", "GZIP_1", "GZIP_2"]:
    for shape, label in [((1, 24), "row"), ((4, 6), "rect"), ((5, 7), "edge")]:
        name = f"{algorithm.lower()}_{label}.fits.fz"
        fits.HDUList([fits.PrimaryHDU(), fits.CompImageHDU(data, compression_type=algorithm, tile_shape=shape)]).writeto(dest/name, overwrite=True)
        with fits.open(dest/name) as hdus:
            values = hdus[1].data.astype(float)
        cases.append(dict(file=name, values=values.ravel().tolist(), shape=list(values.shape), kind="image", expected_supported=True))

rng = np.random.default_rng(20261017)
floats = rng.normal(10, 2, (16, 24)).astype("float32")
floats[2, 3] = np.nan
for algorithm, quantization in [("RICE_1", 4), ("GZIP_1", 0), ("GZIP_2", 0)]:
    name=f"float_{algorithm.lower()}.fits.fz"
    fits.HDUList([fits.PrimaryHDU(), fits.CompImageHDU(floats, compression_type=algorithm, quantize_level=quantization)]).writeto(dest/name,overwrite=True)
    with fits.open(dest/name) as hdus:
        values=hdus[1].data.astype(float)
    cases.append(dict(file=name,values=[float(v) if np.isfinite(v) else None for v in values.ravel()],shape=list(values.shape),kind="image",expected_supported=True))

for dtype in ["uint8", "int32", "uint16"]:
    if dtype == "uint8": array=(np.arange(384).reshape(16,24)%251).astype(dtype)
    elif dtype == "uint16": array=(np.arange(384).reshape(16,24)*149).astype(dtype)
    else:
        array=rng.integers(-(2**31),2**31-1,(16,24),dtype="int32")
        array[0,:4]=[-(2**31),2**31-1,-1,0]
    for algorithm in ["RICE_1","GZIP_1","GZIP_2"]:
        name=f"{dtype}_{algorithm.lower()}.fits.fz"
        fits.HDUList([fits.PrimaryHDU(),fits.CompImageHDU(array,compression_type=algorithm,tile_shape=(5,7))]).writeto(dest/name,overwrite=True)
        with fits.open(dest/name) as hdus: values=hdus[1].data.astype(float)
        cases.append(dict(file=name,values=values.ravel().tolist(),shape=list(values.shape),kind="image",expected_supported=True))
for method in [1,2]:
    array=floats.copy();array[0,0]=0.
    name=f"float_dither{method}.fits.fz"
    fits.HDUList([fits.PrimaryHDU(),fits.CompImageHDU(array,compression_type="RICE_1",quantize_level=4,quantize_method=method,dither_seed=23)]).writeto(dest/name,overwrite=True)
    with fits.open(dest/name) as hdus: values=hdus[1].data.astype(float)
    cases.append(dict(file=name,values=[float(v) if np.isfinite(v) else None for v in values.ravel()],shape=list(values.shape),kind="image",expected_supported=True))
for algorithm in ["HCOMPRESS_1","PLIO_1"]:
    name=f"unsupported_{algorithm.lower()}.fits.fz"
    fits.HDUList([fits.PrimaryHDU(),fits.CompImageHDU(np.abs(data),compression_type=algorithm)]).writeto(dest/name,overwrite=True)
    cases.append(dict(file=name,expected_supported=False))

flux=np.array([1.,3.,-2.,np.nan,5.,7.,4.,2.], dtype="float64")
hdu=fits.PrimaryHDU(flux)
for key,value in dict(CRPIX1=3.,CRVAL1=510.,CDELT1=-2.,CTYPE1="WAVE",CUNIT1="nm",BUNIT="Jy").items():hdu.header[key]=value
hdu.writeto(dest/"spectrum_1d.fits",overwrite=True)
wave=(510+(np.arange(8)+1-3)*-2).tolist()
clean=[float(v) if np.isfinite(v) else None for v in flux]
cases.append(dict(file="spectrum_1d.fits",kind="spectrum",x=wave,values=clean,expected_supported=True))
with gzip.open(dest/"spectrum_1d.fits.gz","wb") as out:out.write((dest/"spectrum_1d.fits").read_bytes())
cases.append(dict(cases[-1],file="spectrum_1d.fits.gz"))
for vector in [False,True]:
    names=["WAVE","FLUX","ERR"]
    values=[np.asarray(wave),flux,np.full(8,.5)]
    cols=[fits.Column(name=n,format="8D" if vector else "D",unit="nm" if n=="WAVE" else "Jy",array=[v] if vector else v) for n,v in zip(names,values)]
    name="spectrum_vector.fits" if vector else "spectrum_table.fits"
    fits.HDUList([fits.PrimaryHDU(),fits.BinTableHDU.from_columns(cols)]).writeto(dest/name,overwrite=True)
    cases.append(dict(file=name,kind="spectrum",x=wave,values=clean,error=[.5]*8,expected_supported=True))
cols=[fits.Column(name="loglam",format="D",array=np.log10(np.arange(5000.,5008.))),fits.Column(name="flux",format="D",array=np.arange(8.)),fits.Column(name="ivar",format="D",array=np.array([4.,0.,4.,4.,4.,4.,4.,4.]))]
fits.HDUList([fits.PrimaryHDU(),fits.BinTableHDU.from_columns(cols)]).writeto(dest/"spectrum_loglam.fits",overwrite=True)
cases.append(dict(file="spectrum_loglam.fits",kind="spectrum",x=list(np.arange(5000.,5008.)),values=list(np.arange(8.)),error=[.5,None,.5,.5,.5,.5,.5,.5],expected_supported=True))
(dest/"spectrum.txt").write_text("# WAVE FLUX ERR\n"+"\n".join(f"{x} {y if y is not None else 'nan'} 5.0D-1" for x,y in zip(wave,clean)),encoding="utf-8")
cases.append(dict(file="spectrum.txt",kind="spectrum",x=wave,values=clean,error=[.5]*8,expected_supported=True))
(dest/"spectrum.csv").write_text("coordinate,intensity\n"+"\n".join(f"{x},{y if y is not None else 'nan'}" for x,y in zip(wave,clean)),encoding="utf-8")
cases.append(dict(file="spectrum.csv",kind="spectrum",x=wave,values=clean,expected_supported=True))
chinese=dest/"中文数据"
chinese.mkdir(exist_ok=True)
image=chinese/"中文图像（测试）.fits"
fits.PrimaryHDU(data).writeto(image,overwrite=True)
image_reference=dict(kind="image",values=data.astype(float).ravel().tolist(),shape=list(data.shape),expected_supported=True)
cases.append(dict(image_reference,file=image.relative_to(dest).as_posix()))
with gzip.open(str(image)+'.gz','wb') as out:out.write(image.read_bytes())
cases.append(dict(image_reference,file=image.relative_to(dest).as_posix()+'.gz'))
for name in ["spectrum_1d.fits","rice_1_row.fits.fz","spectrum.txt"]:
    target=chinese/("观测 星场（测试）"+"".join(Path(name).suffixes))
    target.write_bytes((dest/name).read_bytes())
    original=next(c for c in cases if c['file']==name)
    cases.append(dict(original,file=target.relative_to(dest).as_posix()))
(dest/"reference.json").write_text(json.dumps(cases,indent=2,ensure_ascii=False,allow_nan=False)+"\n",encoding="utf-8")
print(json.dumps(dict(synthetic=True,cases=len(cases))))
