"""Relative image matching tests; controlled in-memory truth and optional NASA FITS.

解析星点仅是明确标注的数值单元测试，不写为演示FITS或冒充真实观测。
--real-image uses the untouched public original and explicitly labeled derived copies.
"""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT/'web/python'))
import fitsviewer_science as fs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--core', type=Path)
    parser.add_argument('--real-image', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    report = {'checks': [], 'real_data_executed': False, 'controlled_data': 'in-memory numerical PSF fixtures only; not demo/observation FITS'}

    def check(name, function):
        started = time.perf_counter()
        try:
            value = function()
            report['checks'].append({'name': name, 'passed': True, 'seconds': time.perf_counter()-started, 'detail': fs.clean(value)})
        except Exception as error:
            import traceback
            traceback.print_exc()
            report['checks'].append({'name': name, 'passed': False, 'error': str(error)})
        (args.output/'image-matching-validation.json').write_text(json.dumps(fs.clean(report), indent=2, ensure_ascii=False, allow_nan=False), encoding='utf-8')

    def lazy():
        code = 'import sys;sys.path.insert(0,sys.argv[1]);import fitsviewer_image_matching;assert all(v not in sys.modules for v in ("numpy","scipy","astropy","photutils"));print("no optional science imports on module initialization")'
        value = subprocess.run([sys.executable, '-c', code, str(ROOT/'web/python')], capture_output=True, text=True, check=True)
        return value.stdout.strip()
    check('lazy_module_imports_no_embedded_scientific_runtime', lazy)

    import numpy as np
    from astropy.io import fits
    from astropy.wcs import WCS, Sip
    import fitsviewer_image_matching as im

    positions = [(float(x), float(y)) for y in (40, 80, 120, 160, 200) for x in (40, 80, 120, 160, 200)]
    shape = (256, 256)
    yy, xx = np.mgrid[:shape[0], :shape[1]]
    rng = np.random.default_rng(20261018)
    noise = rng.normal(0., .2, shape)

    def image(shift=(0., 0.), number=25, variable=None):
        result = np.full(shape, 100.)+noise
        sigma = 3./math.sqrt(8*math.log(2))
        for index, (x, y) in enumerate(positions[:number]):
            amplitude = 300.+10*index
            if variable is not None and index == 0:
                amplitude *= variable
            result += amplitude*np.exp(-.5*(((xx-x-shift[0])/sigma)**2+((yy-y-shift[1])/sigma)**2))
        return result

    def header(shift=(0., 0.), filter_name='r', wcs=True):
        value = fits.Header()
        value['NAXIS'] = 2; value['NAXIS1'] = shape[1]; value['NAXIS2'] = shape[0]
        value['FILTER'] = filter_name; value['EXPTIME'] = 10.; value['BUNIT'] = 'adu'
        value['INSTRUME'] = 'controlled numerical fixture'
        if wcs:
            world = WCS(naxis=2)
            world.wcs.crpix = [128.+shift[0], 128.+shift[1]]
            world.wcs.crval = [150., 30.]
            world.wcs.cdelt = [-1./3600., 1./3600.]
            world.wcs.ctype = ['RA---TAN', 'DEC--TAN']
            value.update(world.to_header())
        return value

    target_path, ref_path = args.output/'target-numerical-fixture.identity', args.output/'reference-numerical-fixture.identity'
    target_path.write_text('Identity marker for controlled in-memory array; not an astronomical image.', encoding='utf-8')
    ref_path.write_text('Identity marker for controlled in-memory array; not an astronomical image.', encoding='utf-8')
    base = image()

    class Fixture:
        def __init__(self, target=None, reference=None, th=None, rh=None, cancel_at=None):
            self.target = 2.5*base+37. if target is None else target
            self.reference = base if reference is None else reference
            self.th = header() if th is None else th
            self.rh = header() if rh is None else rh
            self.cancel_at = cancel_at; self.calls = 0; self.data_reads = 0

        def image(self, p, data=False):
            is_ref = Path(p['path']).resolve() == ref_path.resolve()
            if data:
                self.data_reads += 1
            return (ref_path if is_ref else target_path), 0, (self.rh if is_ref else self.th).copy(), (self.reference if is_ref else self.target) if data else None

        def check_cancelled(self):
            self.calls += 1
            if self.cancel_at is not None and self.calls >= self.cancel_at:
                raise InterruptedError('controlled cancellation')

    params = {'path': str(target_path), 'reference_path': str(ref_path), 'source': 'image matching controlled truth', 'revision': 11,
              'mode': 'pixel', 'assume_aligned': True, 'max_stars': 200, 'min_matches': 5}

    def run(worker=None, **options):
        return im.match_flux(worker or Fixture(), dict(params, **options))

    def scale_truth():
        worker = Fixture()
        worker.target.setflags(write=False); worker.reference.setflags(write=False)
        value = run(worker)
        assert value['status'] == 'ok' and value['used'] >= 20, value
        assert abs(value['factor']-.4) < 1e-10, value['factor']
        assert value['source'] == params['source'] and value['revision'] == 11
        assert value['factor_uncertainty'] is None and not value['provenance']['absolute_calibration']
        assert value['path'] == str(target_path.resolve()) and value['reference_path'] == str(ref_path.resolve())
        assert np.array_equal(worker.target, 2.5*base+37.) and np.array_equal(worker.reference, base)
        return {'factor': value['factor'], 'used': value['used'], 'scatter_fraction': value['scatter_fraction'], 'provenance': value['provenance']}
    check('native_counts_scale_plus_background_offset_truth_readonly_and_identity', scale_truth)

    def exposure_no_double_count():
        th, rh = header(), header(); th['EXPTIME'] = 17.; rh['EXPTIME'] = 105.
        value = run(Fixture(th=th, rh=rh))
        assert abs(value['factor']-.4) < 1e-10
        assert value['provenance']['target_header']['EXPTIME'] == 17.
        return {'factor': value['factor'], 'target_exptime': 17., 'reference_exptime': 105.}
    check('EXPTIME_is_recorded_without_extra_flux_multiplication', exposure_no_double_count)

    def wcs_shift():
        shift = (3., -2.)
        value = run(Fixture(target=2.5*image(shift)+37., th=header(shift)), mode='wcs', assume_aligned=False)
        assert value['status'] == 'ok' and value['used'] >= 20, value
        assert abs(value['factor']-.4) < .001
        matrix = np.asarray(value['mapping']['matrix'])
        assert np.max(np.abs(matrix-np.array([[1., 0., -3.], [0., 1., 2.]]))) < 1e-6, matrix
        assert value['mapping']['status'] == 'ok'
        assert max(row['separation'] for row in value['matches']) < .05
        return {'factor': value['factor'], 'used': value['used'], 'mapping': value['mapping']}
    check('WCS_one_to_one_matching_and_affine_translation_truth', wcs_shift)

    def pixel_explicit():
        try:
            run(assume_aligned=False)
        except ValueError as error:
            assert 'assume_aligned' in str(error)
        else:
            raise AssertionError('Pixel alignment was silently assumed')
        missing = run(Fixture(th=header(wcs=False), rh=header(wcs=False)), mode='wcs')
        assert missing['status'] == 'geometry_unavailable' and missing['factor'] is None
        return {'explicit_required': True, 'missing_wcs': missing['mapping']}
    check('pixel_requires_explicit_registration_assumption_WCS_requires_both_headers', pixel_explicit)

    def filters():
        different = run(Fixture(rh=header(filter_name='g')))
        missing_header = header(); del missing_header['FILTER']
        missing = run(Fixture(rh=missing_header))
        override = run(Fixture(rh=header(filter_name='g')), allow_different_filters=True)
        assert different['status'] == 'filter_mismatch' and different['factor'] is None
        assert missing['status'] == 'filter_unverified' and missing['factor'] is None
        assert override['status'] == 'ok' and any('override' in w for w in override['warnings'])
        return {'different': different['status'], 'missing': missing['status'], 'override_factor': override['factor']}
    check('different_unknown_filters_refused_unless_explicit_display_override', filters)

    def outlier_clipping():
        value = run(Fixture(reference=image(variable=3.)))
        assert value['status'] == 'ok' and value['rejected'] >= 1
        assert abs(value['factor']-.4) < 1e-8
        assert any(not row['used'] and row['reason'] == 'log_ratio_outlier' for row in value['matches'])
        return {'factor': value['factor'], 'matched': value['matched'], 'used': value['used'], 'rejected': value['rejected']}
    check('variable_source_flux_ratio_is_robustly_clipped', outlier_clipping)

    def quality_cuts():
        target = (2.5*image()+37.).copy()
        target[40, 49] = np.nan  # Bad annulus pixel; peak remains detectable and quality cut is exercised.
        target[79:82, 79:82] = 99999.
        # Explicit saturation and bad pixels remove sources, never get replaced/interpolated.
        value = run(Fixture(target=target), saturation=5000.)
        assert value['status'] == 'ok'
        rejected = value['detection']['target']['rejected']
        assert rejected.get('bad_pixels', 0)+rejected.get('bad_pixels_or_small_annulus', 0) >= 1
        assert rejected.get('saturated', 0)+rejected.get('nonstellar_shape', 0) >= 1
        assert abs(value['factor']-.4) < 1e-8
        return {'factor': value['factor'], 'target_rejections': rejected, 'used': value['used']}
    check('bad_pixels_saturation_and_shape_are_excluded', quality_cuts)

    def insufficient():
        empty = run(Fixture(target=np.ones(shape), reference=np.ones(shape)))
        few = run(Fixture(target=2.5*image(number=2)+37., reference=image(number=2)))
        for value in (empty, few):
            assert value['status'] == 'insufficient_matches' and value['factor'] is None
        return {'empty': empty['matched'], 'few': few['matched']}
    check('flat_images_or_too_few_independent_sources_never_yield_factor', insufficient)

    def crowding_and_ambiguity():
        extra = 300*np.exp(-.5*(((xx-48.)/1.274)**2+((yy-40.)/1.274)**2))
        value = run(Fixture(target=2.5*(base+extra)+37., reference=base+extra))
        assert value['status'] == 'ok' and value['detection']['target']['rejected'].get('crowded_nonindependent', 0) > 0
        stars = [{'x': 10., 'y': 10., 'flux': 10., 'screening_snr': 20.}, {'x': 11., 'y': 10., 'flux': 10., 'screening_snr': 20.}]
        pairs, reasons = im._pairs(stars, stars, None, dict(params, match_radius=2.), Fixture(), np)
        assert not pairs and reasons['unmatched_or_ambiguous'] == 2
        return {'crowding_rejections': value['detection']['target']['rejected'], 'ambiguous_pairs': len(pairs)}
    check('crowded_sources_and_nonunique_pairings_are_rejected', crowding_and_ambiguity)

    def no_sky_overlap():
        other = header(); other['CRVAL1'] = 151.
        value = run(Fixture(rh=other), mode='wcs')
        assert value['status'] == 'insufficient_matches' and value['factor'] is None and value['matched'] == 0
        return {'status': value['status'], 'matched': value['matched'], 'mapping_status': value['mapping']['status']}
    check('unrelated_WCS_fields_are_not_matched_at_same_pixel', no_sky_overlap)

    def distorted_geometry():
        th = header(); world = WCS(th)
        a = np.zeros((4, 4)); b = np.zeros((4, 4)); a[2, 0] = .001; b[0, 2] = -.001
        world.sip = Sip(a, b, None, None, world.wcs.crpix)
        world.wcs.ctype = ['RA---TAN-SIP', 'DEC--TAN-SIP']
        th.update(world.to_header(relax=True))
        mapping, _ = im._geometry(th, header(), shape, dict(im._params(params), mode='wcs'), Fixture(), np)
        assert mapping['status'] == 'approximate_only' and mapping['max_residual_pixels'] > .5
        return mapping
    check('distorted_WCS_is_not_mislabeled_affine_registered', distorted_geometry)

    def parameters_and_budget():
        cases = [{'mode': 'pixel', 'assume_aligned': False}, {'aperture_radius': float('nan')}, {'annulus_inner': 4.},
                 {'annulus_outer': 129.}, {'fwhm': 0.}, {'threshold_sigma': 2.}, {'match_radius': 0.},
                 {'min_matches': 2}, {'max_stars': 2001}, {'max_stars': True}, {'allow_different_filters': 1}]
        messages = []
        for options in cases:
            try:
                run(**options)
            except ValueError as error:
                messages.append(str(error))
            else:
                raise AssertionError('Invalid parameters accepted')
        huge = header(); huge['NAXIS1'] = huge['NAXIS2'] = 100000
        worker = Fixture(th=huge)
        try:
            run(worker)
        except ValueError as error:
            assert '16777216' in str(error) and worker.data_reads == 0
        else:
            raise AssertionError('Oversized images loaded')
        return {'validation_messages': messages, 'oversized_data_reads': worker.data_reads}
    check('strict_parameters_and_combined_pixel_cap_before_data_reads', parameters_and_budget)

    def cancellation_and_missing():
        worker = Fixture(cancel_at=12)
        try:
            run(worker)
        except InterruptedError:
            assert worker.calls == 12
        else:
            raise AssertionError('Running cancellation not propagated')
        with patch.object(im, 'require', side_effect=ModuleNotFoundError('Selected environment is missing scipy')):
            try:
                run()
            except ModuleNotFoundError as error:
                assert 'Selected environment' in str(error)
            else:
                raise AssertionError('Missing package not reported')
        return {'cancel_checks': worker.calls, 'missing_package_explicit': True}
    check('running_loop_cancel_and_selected_environment_missing_package', cancellation_and_missing)

    def tiny_units():
        scale = 1e-17
        value = run(Fixture(target=(2.5*base+37.)*scale, reference=base*scale))
        assert value['status'] == 'ok' and abs(value['factor']-.4) < 1e-9
        return {'factor': value['factor'], 'used': value['used']}
    check('small_physical_pixel_units_preserve_relative_ratio', tiny_units)

    if args.real_image:
        def real():
            source = args.real_image.resolve()
            original_hash = hashlib.sha256(source.read_bytes()).hexdigest()
            if args.core:
                os.environ['FITSVIEWER_CORE_BINARY'] = str(args.core.resolve())
            worker = fs.Worker(args.output/'real-cache')
            try:
                _, hdu, real_header, physical = worker.image({'path': str(source)}, data=True)
                # Exact integer shift of real pixels, not a generated demonstration field.
                shift_x, shift_y, scale = 3, -2, 1.75
                finite = physical[np.isfinite(physical)]
                offset = float(np.median(finite))*.23
                derived = np.full_like(physical, np.nan, dtype=float)
                src_y = slice(max(0, -shift_y), min(physical.shape[0], physical.shape[0]-shift_y))
                src_x = slice(max(0, -shift_x), min(physical.shape[1], physical.shape[1]-shift_x))
                dst_y = slice(max(0, shift_y), min(physical.shape[0], physical.shape[0]+shift_y))
                dst_x = slice(max(0, shift_x), min(physical.shape[1], physical.shape[1]+shift_x))
                derived[dst_y, dst_x] = scale*physical[src_y, src_x]+offset
                th = real_header.copy()
                # `physical` already includes source BSCALE/BZERO; float derivative must not apply them twice.
                for scaling_key in ('BSCALE', 'BZERO', 'BLANK', 'CHECKSUM', 'DATASUM'):
                    th.remove(scaling_key, ignore_missing=True, remove_all=True)
                if 'CRPIX1' in th and 'CRPIX2' in th:
                    th['CRPIX1'] += shift_x; th['CRPIX2'] += shift_y
                    mode = 'wcs'; assumed = False
                else:
                    # Flux-only test uses a separate zero-shift derived copy when WCS is absent.
                    derived = scale*physical+offset; mode = 'pixel'; assumed = True
                th['HISTORY'] = 'PRIVATE QA DERIVED COPY: native physical pixels scaled and offset, optional exact integer shift; not an independent observation.'
                derivative = args.output/'real-image-controlled-derived.fits'
                fits.PrimaryHDU(derived, header=th).writeto(derivative, overwrite=False)
                attempts = []
                best = None
                for fwhm in (3., 5., 7.):
                    options = {'path': str(derivative), 'hdu': 0, 'reference_path': str(source), 'reference_hdu': hdu,
                        'source': 'real NASA input plus explicitly labeled controlled derivative', 'revision': 1,
                        'mode': mode, 'assume_aligned': assumed, 'fwhm': fwhm, 'threshold_sigma': 5.,
                        'aperture_radius': max(5., fwhm*1.7), 'annulus_inner': max(8., fwhm*2.2),
                        'annulus_outer': max(12., fwhm*3.), 'min_matches': 3}
                    value = im.match_flux(worker, options)
                    attempts.append({'fwhm': fwhm, 'status': value['status'], 'matched': value['matched'], 'used': value['used'], 'factor': value['factor']})
                    if value['status'] == 'ok':
                        best = value; break
                assert original_hash == hashlib.sha256(source.read_bytes()).hexdigest()
                assert best is not None, {'attempts': attempts, 'note': 'The real sample did not support enough independent stellar references; do not fabricate extra stars.'}
                assert abs(best['factor']-1./scale) < .025
                if mode == 'wcs' and best['mapping']['status'] == 'ok':
                    assert abs(best['mapping']['matrix'][0][2]+shift_x) < .05
                    assert abs(best['mapping']['matrix'][1][2]+shift_y) < .05
                report['real_data_executed'] = True
                return {'original_path': str(source), 'original_sha256': original_hash, 'derivative_path': str(derivative),
                        'derivative_sha256': hashlib.sha256(derivative.read_bytes()).hexdigest(), 'controlled_scale': scale,
                        'controlled_offset': offset, 'controlled_shift': [shift_x, shift_y] if mode == 'wcs' else [0, 0],
                        'attempts': attempts, 'result': best, 'note': 'Real input pixels retained; derived copy clearly labeled; no stars injected.'}
            finally:
                worker.close()
        check('real_NASA_native_file_plus_labeled_scale_offset_shift_derivative', real)
    else:
        report['real_data_note'] = 'Not executed: pass --real-image and optionally --core. In-memory fixtures are analytic numerical tests only.'
        (args.output/'image-matching-validation.json').write_text(json.dumps(fs.clean(report), indent=2, ensure_ascii=False, allow_nan=False), encoding='utf-8')
    passed = sum(row['passed'] for row in report['checks'])
    print(json.dumps({'passed': passed, 'checks': len(report['checks']), 'real_data_executed': report['real_data_executed'], 'output': str(args.output)}))
    return 0 if all(row['passed'] for row in report['checks']) else 1


if __name__ == '__main__':
    raise SystemExit(main())
