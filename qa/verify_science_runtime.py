"""Independent native-core numerical checks; real inputs are always read-only.

Synthetic codec/photometry fixtures are explicitly labelled and kept in --output.
The oracle is Astropy/CFITSIO or analytic sums, never FitsViewer display samples.
"""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import traceback

import numpy as np
from astropy.io import fits

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / 'web/python'))
from fitsviewer_science import Worker, clean


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--core', type=Path, required=True)
    parser.add_argument('--data', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    fixtures = args.output / 'synthetic-fixtures'
    fixtures.mkdir(exist_ok=True)
    os.environ['FITSVIEWER_CORE_BINARY'] = str(args.core.resolve())
    worker = Worker(args.output / 'cache')
    report = {'checks': [], 'oracle': 'Astropy/CFITSIO and independent analytic sums',
              'core_sha256': hashlib.sha256(args.core.read_bytes()).hexdigest()}
    inputs = [p for p in args.data.iterdir() if p.suffix.lower() in ('.fits', '.fit', '.fz')]
    identities = {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in inputs}

    def check(name, operation):
        try:
            detail = operation()
            report['checks'].append({'name': name, 'passed': True, 'detail': clean(detail)})
        except Exception as error:
            traceback.print_exc()
            report['checks'].append({'name': name, 'passed': False, 'error': str(error)})
        (args.output / 'runtime-validation.json').write_text(json.dumps(clean(report), indent=2, allow_nan=False), encoding='utf-8')

    def image(path, hdu):
        packet = worker.dataset({'path': str(path), 'hdu': hdu})
        # Astropy's default scaling of int16 is float32, whereas the core
        # intentionally evaluates physical values in float64. Read storage
        # samples independently, then apply the original FITS scaling once.
        with fits.open(path, memmap=False, do_not_scale_image_data=True, uint=False) as hdus:
            raw = np.asarray(hdus[hdu].data)
            header = hdus[hdu].header
            expected = raw.astype(float) * header.get('BSCALE', 1.) + header.get('BZERO', 0.)
            if raw.dtype.kind in 'iu' and 'BLANK' in header:
                expected[raw == header['BLANK']] = np.nan
        actual = packet['values']
        np.testing.assert_allclose(actual, expected, rtol=0, atol=0, equal_nan=True)
        return {'hdu': hdu, 'shape': list(expected.shape), 'pixels': int(expected.size),
                'source_bitpix': packet['meta']['bitpix'], 'physical_max_abs_error': 0.}

    for path in inputs:
        with fits.open(path, memmap=False) as hdus:
            selected = [i for i, h in enumerate(hdus) if isinstance(h, (fits.PrimaryHDU, fits.ImageHDU, fits.CompImageHDU)) and len(h.shape or ()) == 2]
        for hdu in selected:
            check('real_native_image:' + path.name + ':' + str(hdu), lambda p=path, h=hdu: image(p, h))

    yy, xx = np.mgrid[:173, :307]
    integer = ((xx * 13 + yy * 7) % 30000 - 15000).astype(np.int16)
    integer[19, 23] = -32768
    scaled = fits.PrimaryHDU(integer)
    scaled.header['BSCALE'] = .125
    scaled.header['BZERO'] = 32768.25
    scaled.header['BLANK'] = -32768
    scaled.header['BUNIT'] = 'adu'
    path = fixtures / 'synthetic-scaled-blank.fits'
    scaled.writeto(path, overwrite=True)
    check('scaled_integer_once_with_blank', lambda: image(path, 0))
    gz = fixtures / 'synthetic-scaled-blank.fits.gz'
    with gzip.open(gz, 'wb') as stream:
        stream.write(path.read_bytes())
    check('gzip_scaled_integer_once_with_blank', lambda: image(gz, 0))

    for codec in ('RICE_1', 'GZIP_1', 'GZIP_2'):
        for dtype in ('uint8', 'int16', 'int32', 'float32', 'float64'):
            if dtype == 'uint8':
                values = ((xx + yy) % 251).astype(dtype)
            elif dtype.startswith('int'):
                values = integer.astype(dtype)
            else:
                values = (np.sin(xx / 11.) + np.cos(yy / 17.) + 100.).astype(dtype)
                values[19, 23] = np.nan
                values[21, 27] = 0.
            target = fixtures / ('synthetic-' + codec + '-' + dtype + '.fits.fz')
            # Non-divisible source dimensions and >10000-pixel source tiles check
            # partial edges and wrapping of CFITSIO's subtractive-dither sequence.
            options = {'tile_shape': (71, 200), 'compression_type': codec}
            if dtype.startswith('float'):
                options.update(quantize_level=16 if codec == 'RICE_1' else 0,
                               quantize_method=2, dither_seed=37)
            hdu = fits.CompImageHDU(values, **options)
            fits.HDUList([fits.PrimaryHDU(), hdu]).writeto(target, overwrite=True)
            check('codec:' + codec + ':' + dtype, lambda p=target: image(p, 1))

    def measure(params):
        completed = subprocess.run([str(args.core.resolve()), '--measure-json'],
                                   input=json.dumps(params), text=True, capture_output=True, timeout=40)
        if completed.returncode:
            raise RuntimeError(completed.stderr)
        return json.loads(completed.stdout)

    def aperture_checks():
        raw = np.full((64, 64), 10, dtype=np.int16)
        raw[19:22, 19:22] = 40
        science = fits.PrimaryHDU(raw)
        science.header['BSCALE'] = .5
        science.header['BZERO'] = 100.
        science.header['BUNIT'] = 'adu'
        sigma = np.full(raw.shape, 2.)
        target = fixtures / 'synthetic-aperture-errors.fits'
        fits.HDUList([science, fits.ImageHDU(sigma, name='SIGMA'),
                      fits.ImageHDU(sigma ** 2, name='VAR'),
                      fits.ImageHDU(1 / sigma ** 2, name='IVAR')]).writeto(target, overwrite=True)
        rectangle = lambda center: {'kind': 'rectangle', 'center': center, 'size': [3., 3.], 'angle_deg': 0.}
        aperture = {'name': 'analytic 9-pixel source', 'shape': rectangle([20., 20.]),
                    'background': rectangle([30., 30.]), 'exclude': []}
        params = {'path': str(target), 'hdu': 0, 'apertures': [aperture]}
        missing = measure(params)['measurements'][0]
        assert missing['total_error'] is None and missing['snr'] is None
        result = []
        for hdu, kind in ((1, 'sigma'), (2, 'variance'), (3, 'ivar')):
            value = measure(dict(params, error_hdu=hdu, error_kind=kind))['measurements'][0]
            np.testing.assert_allclose([value['aperture_sum'], value['effective_area'],
                                       value['background_per_pixel'], value['net_flux'],
                                       value['total_error'] ** 2], [1080., 9., 105., 135., 72.], rtol=0, atol=1e-10)
            result.append({'error_kind': kind, 'measurement': value})
        return result
    check('aperture_scaled_physical_values_sigma_variance_ivar_and_no_invented_noise', aperture_checks)

    def real_spectrum():
        path = args.data / 'spec-0751-52251-0160.fits'
        packet = worker.dataset({'path': str(path), 'hdu': 1, 'x_column': 1, 'y_column': 0, 'error_column': 2})
        with fits.open(path, memmap=False) as hdus:
            table = hdus[1].data
            # Double precision exponentiation of the original stored log-lambda.
            expected_x = 10. ** np.asarray(table['loglam'], dtype=float)
            expected_y = np.asarray(table['flux'], dtype=float)
            ivar = np.asarray(table['ivar'], dtype=float)
            mask = np.asarray(table['and_mask']) != 0
            expected_y[mask] = np.nan
        expected_error = np.full(len(ivar), np.nan)
        use = np.isfinite(ivar) & (ivar > 0)
        expected_error[use] = 1. / np.sqrt(ivar[use])
        np.testing.assert_allclose(packet['x'], expected_x, rtol=2e-15, atol=0)
        np.testing.assert_allclose(packet['values'], expected_y, rtol=0, atol=0, equal_nan=True)
        np.testing.assert_allclose(packet['error'], expected_error, rtol=2e-15, atol=0, equal_nan=True)
        return {'samples': len(expected_x), 'quality_masked_samples': int(mask.sum()),
                'unit': packet['meta']['spectrum']['x_unit'],
                'error_kind': packet['meta']['spectrum']['uncertainty_kind']}
    check('real_SDSS_original_flux_loglam_and_inverse_variance', real_spectrum)

    def real_linear_spectrum(name):
        path = args.data / name
        packet = worker.dataset({'path': str(path), 'hdu': 0, 'error_hdu': 1, 'error_kind': 'variance'})
        with fits.open(path, memmap=False) as hdus:
            header = hdus[0].header
            flux = np.asarray(hdus[0].data, dtype=float)
            variance = np.asarray(hdus[1].data, dtype=float)
            wave = header['CRVAL1'] + (np.arange(len(flux)) + 1 - header['CRPIX1']) * header['CDELT1']
        sigma = np.full(len(variance), np.nan)
        valid = np.isfinite(variance) & (variance >= 0)
        sigma[valid] = np.sqrt(variance[valid])
        np.testing.assert_allclose(packet['values'], flux, rtol=0, atol=0, equal_nan=True)
        np.testing.assert_allclose(packet['x'], wave, rtol=0, atol=0)
        np.testing.assert_allclose(packet['error'], sigma, rtol=2e-15, atol=0, equal_nan=True)
        return {'samples': len(flux), 'science_hdu': 0, 'variance_hdu': 1,
                'axis_unit': packet['meta']['spectrum']['x_unit']}
    for name in ('SN2011fe-11feM003.fit', 'SN2011fe-11feM152.fit'):
        check('real_linear_spectrum:' + name, lambda n=name: real_linear_spectrum(n))

    def real_calspec():
        path = args.data / 'alpha_lyr_stis_011.fits'
        packet = worker.dataset({'path': str(path), 'hdu': 1, 'x_column': 0, 'y_column': 1,
                                 'error_column': 2, 'error_kind': 'sigma'})
        with fits.open(path, memmap=False) as hdus:
            table = hdus[1].data
            for key, column in (('x', 'WAVELENGTH'), ('values', 'FLUX'), ('error', 'STATERROR')):
                np.testing.assert_allclose(packet[key], np.asarray(table[column], dtype=float), rtol=0, atol=0, equal_nan=True)
        return {'samples': len(packet['x']), 'error_column': 'STATERROR',
                'scope': 'Statistical sigma explicitly selected; SYSERROR is not silently combined.'}
    check('real_CALSPEC_WAVELENGTH_FLUX_STATERROR', real_calspec)

    def real_wcs():
        from astropy.coordinates import FK5, SkyCoord
        from astropy.time import Time
        from astropy.wcs import WCS
        from astropy.wcs.utils import pixel_to_skycoord
        import astropy.units as u
        path = args.data / 'M6707HH.fits'
        header = fits.getheader(path)
        points = np.array([[0., 0.], [1058., 1058.], [529., 529.], [123.25, 789.75]])
        value = worker.wcs({'path': str(path), 'hdu': 0, 'points': points.tolist()})
        oracle = pixel_to_skycoord(points[:, 0], points[:, 1], WCS(header).celestial, origin=0, mode='all').transform_to(FK5(equinox=Time('J2000')))
        actual = SkyCoord([p['ra_deg'] for p in value['points']] * u.deg,
                          [p['dec_deg'] for p in value['points']] * u.deg, frame=FK5(equinox=Time('J2000')))
        error = float(actual.separation(oracle).arcsec.max())
        assert value['coordinate_origin'] == 0 and error < 1e-8
        return {'points': points.tolist(), 'max_separation_arcsec': error, 'target_frame': 'FK5 J2000'}
    check('real_M67_WCS_pixel_centers_and_FK5_J2000', real_wcs)

    def real_aperture():
        path = args.data / 'M6707HH.fits'
        with fits.open(path, memmap=False) as hdus:
            values = np.asarray(hdus[0].data, dtype=float)
        expected = float(np.sum(values[528:531, 528:531]))
        aperture = {'name': 'real 9-pixel direct sum', 'shape': {'kind': 'rectangle',
                    'center': [529., 529.], 'size': [3., 3.], 'angle_deg': 0.}, 'background': None, 'exclude': []}
        value = measure({'path': str(path), 'hdu': 0, 'apertures': [aperture]})['measurements'][0]
        assert value['effective_area'] == 9. and value['aperture_sum'] == expected
        assert value['total_error'] is None and value['snr'] is None
        return {'independent_original_pixel_sum': expected, 'measurement': value}
    check('real_M67_original_pixel_aperture_sum', real_aperture)

    worker.close()
    report['source_files_unchanged'] = all(hashlib.sha256(p.read_bytes()).hexdigest() == identities[p.name] for p in inputs)
    report['source_sha256'] = identities
    report['passed'] = report['source_files_unchanged'] and all(row['passed'] for row in report['checks'])
    (args.output / 'runtime-validation.json').write_text(json.dumps(clean(report), indent=2, allow_nan=False), encoding='utf-8')
    print(json.dumps({'passed': report['passed'], 'checks': len(report['checks']),
                      'failures': [row['name'] for row in report['checks'] if not row['passed']]}))
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
