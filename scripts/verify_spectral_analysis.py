"""Numerical truth checks and optional real SDSS shared-packet regression.

受控解析真值不冒充真实观测；真实 SDSS 验证需要显式 --core/--real-spectrum。
No data download, package installation, GUI mutation or release build is performed.
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
    parser.add_argument('--real-spectrum', type=Path)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    report = {'checks': [], 'real_data_executed': False, 'scope': 'read-only full-source spectral analysis'}

    def check(name, operation):
        started = time.perf_counter()
        try:
            result = operation()
            report['checks'].append({'name': name, 'passed': True, 'seconds': time.perf_counter()-started, 'detail': fs.clean(result)})
        except Exception as error:
            import traceback
            traceback.print_exc()
            report['checks'].append({'name': name, 'passed': False, 'error': str(error)})
        (args.output/'spectral-analysis-validation.json').write_text(json.dumps(fs.clean(report), ensure_ascii=False, indent=2, allow_nan=False), encoding='utf-8')

    def lazy():
        source = 'import sys; sys.path.insert(0,sys.argv[1]); import fitsviewer_spectral_analysis; assert "numpy" not in sys.modules and "scipy" not in sys.modules; print("no scientific import at module initialization")'
        completed = subprocess.run([sys.executable, '-c', source, str(ROOT/'web/python')], capture_output=True, text=True, check=True)
        return completed.stdout.strip()
    check('module_initialization_is_lazy', lazy)

    import numpy as np
    from scipy.special import voigt_profile
    import fitsviewer_spectral_analysis as sa
    x = np.linspace(4800., 5100., 1501)
    amplitude, center, sigma = -.5, 4950., 3.
    y = 1.+amplitude*np.exp(-.5*((x-center)/sigma)**2)
    errors = np.full(len(x), .01)
    defaults = {'action': 'measure', 'unit': 'Angstrom', 'source': 'controlled analytic truth', 'revision': 7,
                'region': [4920., 4980.], 'continuum_windows': [[4800., 4880.], [5020., 5100.]], 'mc_samples': 0}

    class Fixture:
        def __init__(self, wave=x, flux=y, error=errors, meta=None, cancel_at=None):
            self.packet = {'x': wave, 'values': flux, 'meta': {'spectrum': meta or {'x_label': 'Wavelength', 'x_unit': 'Angstrom', 'y_unit': ''}}}
            if error is not None:
                self.packet['error'] = error
            self.calls = 0
            self.cancel_at = cancel_at

        def check_cancelled(self):
            self.calls += 1
            if self.cancel_at is not None and self.calls >= self.cancel_at:
                raise RuntimeError('controlled cancellation')

        def dataset(self, params):
            return self.packet

    def run(worker=None, **overrides):
        return sa.analyze(worker or Fixture(), dict(defaults, **overrides))

    def metric(result, key):
        return next(row for row in result['measurements']['metrics'] if row['key'] == key)

    def analytic_measure():
        value = run(redshift=.1)
        expected = -amplitude*sigma*math.sqrt(2*math.pi)
        ew = metric(value, 'equivalent_width_observed')
        assert abs(ew['value']-expected) < 1e-9, (ew, expected)
        assert abs(metric(value, 'net_flux_observed')['value']+expected) < 1e-9
        assert abs(metric(value, 'equivalent_width_rest')['value']-expected/1.1) < 1e-9
        assert abs(metric(value, 'centroid_observed')['value']-center) < 1e-9
        assert abs(metric(value, 'fwhm_observed')['value']-sigma*math.sqrt(8*math.log(2))) < .003
        assert metric(value, 'minimum_wavelength_observed')['value'] == center
        assert metric(value, 'minimum_wavelength_observed')['uncertainty'] is None
        assert value['source'] == defaults['source'] and value['revision'] == 7
        assert value['provenance']['flux_unit'] == 'source flux (unit unknown)'
        return {'analytic_absorption_EW': expected, 'metrics': value['measurements']['metrics']}
    check('analytic_absorption_EW_flux_centroid_FWHM_rest_and_source_identity', analytic_measure)

    def emission():
        value = run(Fixture(flux=2.-y))
        assert metric(value, 'equivalent_width_observed')['value'] < 0
        assert metric(value, 'net_flux_observed')['value'] > 0
        assert abs(metric(value, 'centroid_observed')['value']-center) < 1e-9
        return value['measurements']['metrics']
    check('emission_EW_sign_and_positive_profile_moments', emission)

    def continuum_polynomial():
        xx = (x-4950.)/150.
        continuum = 2.+.3*xx+.1*xx*xx
        instance = Fixture(flux=continuum-.4*np.exp(-.5*((x-4950.)/3.)**2))
        result = run(instance, action='continuum', continuum_method='poly', continuum_degree=2)
        baseline = next(v for v in result['series'] if v['kind'] == 'continuum')
        assert np.max(np.abs(np.asarray(baseline['y'])-continuum)) < 2e-14
        assert result['measurements']['continuum']['weights'] == '1/source_sigma'
        assert result['measurements']['continuum']['excluded_region'] == defaults['region']
        assert {v['kind'] for v in result['series']} == {'continuum', 'normalized', 'subtracted'}
        return result['measurements']
    check('weighted_polynomial_continuum_excludes_line_region', continuum_polynomial)

    def continuum_empty_region():
        value = run(Fixture(flux=np.full(len(x), 3.)), action='continuum', region=None, continuum_windows=[], continuum_method='poly', continuum_degree=0)
        assert value['measurements']['continuum']['degree'] == 0
        assert value['measurements']['continuum']['samples'] == len(x)
        return value['measurements']
    check('all_spectrum_constant_continuum_without_line_region', continuum_empty_region)

    def descending_and_units():
        asc = run()
        descending = run(Fixture(x[::-1], y[::-1], errors[::-1]))
        assert descending['measurements']['metrics'] == asc['measurements']['metrics']
        assert descending['series'][0]['x'][0] > descending['series'][0]['x'][-1]
        assert descending['measurements']['minimum_source_index'] == len(x)-1-int(np.argmin(y))
        observed = []
        for name, factor in [('nm', 10.), ('um', 10000.)]:
            value = run(Fixture(x/factor), unit=name, region=[v/factor for v in defaults['region']],
                        continuum_windows=[[a/factor, b/factor] for a, b in defaults['continuum_windows']])
            assert abs(metric(value, 'equivalent_width_observed')['value']*factor-metric(asc, 'equivalent_width_observed')['value']) < 1e-9
            observed.append(metric(value, 'equivalent_width_observed'))
        return {'descending': True, 'units': observed}
    check('descending_order_source_indices_and_nm_um_equivalence', descending_and_units)

    def error_and_seed():
        first = run(mc_samples=48, seed=17)
        second = run(mc_samples=48, seed=17)
        third = run(mc_samples=48, seed=18)
        assert first['measurements']['metrics'] == second['measurements']['metrics']
        assert metric(first, 'equivalent_width_observed')['uncertainty'] > 0
        assert metric(first, 'equivalent_width_observed')['uncertainty'] != metric(third, 'equivalent_width_observed')['uncertainty']
        assert first['measurements']['uncertainty']['includes_continuum_refit']
        assert not first['measurements']['uncertainty']['includes_covariance']
        return {'first': metric(first, 'equivalent_width_observed'), 'different_seed': metric(third, 'equivalent_width_observed')}
    check('independent_sigma_MC_fixed_seed_and_continuum_refit', error_and_seed)

    def unavailable_errors():
        no_error = run(Fixture(error=None), mc_samples=16)
        ignored = run(no_error=True, mc_samples=16)
        broken = errors.copy(); broken[(x > 4949.) & (x < 4951.)] = 0
        invalid = run(Fixture(error=broken), mc_samples=16)
        for value in (no_error, ignored, invalid):
            assert all(row['uncertainty'] is None for row in value['measurements']['metrics'])
            assert metric(value, 'equivalent_width_observed')['uncertainty_status'] == 'not_available'
        return {'absent': no_error['measurements']['uncertainty'], 'invalid': invalid['measurements']['uncertainty']}
    check('missing_invalid_and_explicitly_disabled_sigma_never_invents_errors', unavailable_errors)

    def incomplete():
        broken = y.copy(); broken[(x > 4949.) & (x < 4951.)] = np.nan
        nan = run(Fixture(flux=broken))
        masked = run(masks=[[4949., 4951.]])
        gap_x = x.copy(); gap_x[x >= 4950.] += 5.
        gap = run(Fixture(wave=gap_x))
        for value in (nan, masked, gap):
            assert value['measurements']['status'] == 'incomplete_region'
            assert value['measurements']['metrics'] == [] and value['series'] == []
        return {'nan': nan['measurements'], 'mask': masked['measurements'], 'coordinate_gap': gap['measurements']}
    check('measurement_refuses_nan_mask_and_large_coordinate_gap', incomplete)

    def invalid_widths():
        truncated = run(region=[4948., 4952.])
        assert metric(truncated, 'fwhm_observed')['status'] == 'truncated'
        two = 1.-.5*np.exp(-.5*((x-4943.)/2.)**2)-.5*np.exp(-.5*((x-4957.)/2.)**2)
        value = run(Fixture(flux=two))
        assert metric(value, 'fwhm_observed')['status'] == 'multiple_peaks'
        negative = run(Fixture(flux=-y))
        assert metric(negative, 'equivalent_width_observed')['value'] is None
        return {'truncated': metric(truncated, 'fwhm_observed'), 'multiple': metric(value, 'fwhm_observed'), 'negative_continuum': metric(negative, 'equivalent_width_observed')}
    check('undefined_continuum_truncated_and_multipeak_FWHM_are_not_numbers', invalid_widths)

    def smoothing():
        result = []
        for method in ('boxcar', 'gaussian', 'savgol'):
            value = run(Fixture(flux=np.full(len(x), 2.)), action='smooth', smooth_method=method, masks=[[4948., 4952.]])
            smooth = value['series'][0]
            assert None in smooth['y']
            assert all(abs(v-2.) < 1e-12 for v in smooth['y'] if v is not None)
            result.append(value['measurements'])
        return result
    check('all_smoothing_kernels_preserve_constant_and_mask_gaps', smoothing)

    def nonuniform():
        wave = x.copy(); wave[500:] += np.arange(len(wave)-500)*.03
        for method in ('gaussian', 'savgol'):
            try:
                run(Fixture(wave=wave), action='smooth', smooth_method=method)
            except ValueError as error:
                assert 'uniform' in str(error)
            else:
                raise AssertionError('Nonuniform grid accepted')
        value = run(Fixture(wave=wave), action='smooth', smooth_method='boxcar')
        assert value['measurements']['status'] == 'ok'
        return value['warnings']
    check('nonuniform_grid_rejects_Gaussian_SG_allows_sample_boxcar', nonuniform)

    def gaussian_fit():
        baseline = 2.+.001*(x-4950.)
        flux = baseline-.6*np.exp(-.5*((x-4944.)/2.5)**2)-.25*np.exp(-.5*((x-4960.)/3.5)**2)
        initial = [{'amplitude': -.5, 'center': 4943., 'sigma': 2.}, {'amplitude': -.2, 'center': 4961., 'sigma': 4.}]
        value = run(Fixture(flux=flux), action='fit', components=initial)
        measured = value['measurements']
        assert measured['status'] == 'ok' and measured['fit']['weighted']
        for entry, truth in zip(measured['components'], [(-.6, 4944., 2.5), (-.25, 4960., 3.5)]):
            assert np.max(np.abs(np.array([entry[k] for k in ('amplitude', 'center', 'sigma')])-truth)) < 1e-7
            assert entry['uncertainty'] is not None
        assert measured['fit']['covariance_rank'] == 6
        assert measured['fit']['covariance_status'] == 'conditional_on_fixed_continuum'
        assert {s['kind'] for s in value['series']} == {'model', 'component', 'residual'}
        assert max(abs(v) for v in value['series'][-1]['y'] if v is not None) < 1e-8
        return measured
    check('two_bounded_Gaussians_truth_residual_and_conditional_covariance', gaussian_fit)

    def tiny_flux_fit():
        scale = 1e-17
        value = run(Fixture(flux=y*scale, error=errors*scale), action='fit', components=[{'amplitude': -.45*scale, 'center': 4949., 'sigma': 2.5}])
        fitted = value['measurements']['components'][0]
        assert abs(fitted['amplitude']/scale-amplitude) < 1e-8
        assert abs(fitted['center']-center) < 1e-7
        assert abs(fitted['sigma']-sigma) < 1e-7
        return fitted
    check('astrophysical_tiny_flux_fit_is_well_scaled', tiny_flux_fit)

    def voigt_truth():
        g, l, amp = 2.4, 1.3, .8
        gs = g/math.sqrt(8*math.log(2))
        profile = amp*voigt_profile(x-center, gs, l/2)/voigt_profile(0., gs, l/2)
        value = run(Fixture(flux=profile), action='fit', fit_profile='voigt', fit_include_continuum=False,
                    components=[{'amplitude': .7, 'center': 4949.5, 'fwhm_g': 2., 'fwhm_l': 1.}])
        fitted = value['measurements']['components'][0]
        assert np.max(np.abs(np.array([fitted[k] for k in ('amplitude', 'center', 'fwhm_g', 'fwhm_l')])-[amp, center, g, l])) < 1e-7
        unweighted = run(Fixture(flux=profile, error=None), action='fit', fit_profile='voigt', fit_include_continuum=False,
                    components=[{'amplitude': .7, 'center': 4949.5, 'fwhm_g': 2., 'fwhm_l': 1.}])
        assert unweighted['measurements']['fit']['covariance'] is None
        assert unweighted['measurements']['components'][0]['uncertainty'] is None
        return {'weighted': fitted, 'unweighted_status': unweighted['measurements']['fit']['covariance_status']}
    check('peak_normalized_Voigt_truth_and_no_sigma_no_formal_errors', voigt_truth)

    def fit_gaps_and_bounds():
        value = run(action='fit', masks=[[4949., 4950.]], components=[{'amplitude': -.4, 'center': 4949., 'sigma': 2.5}])
        assert value['measurements']['status'] == 'ok' and None in value['series'][0]['y']
        pinned = run(action='fit', components=[{'amplitude': -.3, 'center': 4949., 'sigma': 3., 'bounds': {'amplitude': [-.4, -.1]}}])
        assert 'component 1.amplitude' in pinned['measurements']['fit']['boundary_parameters']
        return {'gapped_samples': value['measurements']['fit']['samples'], 'boundary': pinned['measurements']['fit']['boundary_parameters']}
    check('fit_omits_masked_samples_and_reports_parameter_boundaries', fit_gaps_and_bounds)

    def preview():
        wave = np.linspace(4000., 8000., 100_001)
        flux = np.ones(len(wave)); flux[12345] = 123.; flux[56789] = -91.; flux[35000:37000] = np.nan
        worker = Fixture(wave, flux, None)
        value = run(worker, action='continuum', region=None, continuum_windows=[], continuum_output='subtracted')
        series = value['series'][0]
        assert len(series['x']) <= 8192 and None in series['y']
        assert max(v for v in series['y'] if v is not None) > 120
        assert min(v for v in series['y'] if v is not None) < -90
        assert value['provenance']['source_samples'] == len(wave)
        assert np.isnan(worker.packet['values'][35001]) and worker.packet['values'][12345] == 123
        return {'source_samples': len(wave), 'display_samples': len(series['x']), 'gaps': series['y'].count(None)}
    check('full_source_computation_preview_extrema_gaps_and_readonly_inputs', preview)

    def immutable_arrays_and_identity():
        wave, flux, error = x.copy(), y.copy(), errors.copy()
        for array in (wave, flux, error):
            array.setflags(write=False)
        for action in ('measure', 'continuum', 'smooth', 'fit'):
            options = {'action': action, 'mc_samples': 8}
            if action == 'fit':
                options['components'] = [{'amplitude': -.4, 'center': 4949., 'sigma': 2.5}]
            value = run(Fixture(wave, flux, error), **options)
            assert value['measurements']['status'] == 'ok'
            assert np.array_equal(flux, y) and np.array_equal(wave, x) and np.array_equal(error, errors)
        marker = args.output/'fingerprint-fixture.dat'
        marker.write_bytes(b'controlled identity fixture, not an astronomical spectrum\n')
        value = run(Fixture(wave, flux, error), path=str(marker))
        identity = value['provenance']['source_file']
        assert identity['sha256'] == hashlib.sha256(marker.read_bytes()).hexdigest()
        assert identity['size_bytes'] == marker.stat().st_size and identity['mtime_ns'] == marker.stat().st_mtime_ns
        assert identity['status'] == 'sha256_and_stat'
        return identity
    check('source_arrays_can_be_readonly_and_on_disk_source_is_fingerprinted', immutable_arrays_and_identity)

    def validation():
        options = [{'region': None}, {'unit': 'pixel'}, {'redshift': -1.}, {'redshift': float('nan')},
                   {'continuum_degree': 6}, {'mc_samples': 513}, {'mc_samples': True}, {'seed': -1},
                   {'masks': [[2, 1]]}, {'smooth_window': 8}, {'smooth_sigma': 0}, {'smooth_polyorder': 5, 'smooth_window': 3},
                   {'action': 'fit', 'components': []}, {'action': 'fit', 'components': [{'amplitude': -.4, 'center': 4949., 'sigma': -1.}]},
                   {'action': 'fit', 'components': [{'amplitude': -.4, 'center': 4949., 'sigma': 2., 'bounds': {'sigma': [0., 5.]}}]}]
        messages = []
        for options_entry in options:
            try:
                run(**options_entry)
            except ValueError as error:
                messages.append(str(error))
            else:
                raise AssertionError('Invalid options accepted: '+str(options_entry))
        repeated = x.copy(); repeated[100] = repeated[99]
        for worker in [Fixture(wave=repeated), Fixture(wave=np.ones(len(x))), Fixture(meta={'x_label': 'Frequency', 'x_unit': 'Hz'})]:
            try:
                run(worker)
            except ValueError as error:
                messages.append(str(error))
            else:
                raise AssertionError('Invalid axis accepted')
        return messages
    check('strict_parameter_axis_width_bound_and_integer_validation', validation)

    def cancellation():
        measured = Fixture(cancel_at=12)
        fitted = Fixture(cancel_at=12)
        for worker, options in [(measured, {'mc_samples': 64}),
                                (fitted, {'action': 'fit', 'components': [{'amplitude': -.4, 'center': 4948., 'sigma': 2.}]})]:
            try:
                run(worker, **options)
            except RuntimeError as error:
                assert str(error) == 'controlled cancellation'
                assert worker.calls == 12
            else:
                raise AssertionError('Cancellation not propagated')
        return {'MC_cancel_checks': measured.calls, 'fit_cancel_checks': fitted.calls}
    check('mid_MC_and_optimizer_cancellation_propagates_without_partial_result', cancellation)

    def missing_package():
        with patch.object(sa, 'require', side_effect=ModuleNotFoundError('Selected environment is missing scipy')):
            try:
                run()
            except ModuleNotFoundError as error:
                assert 'Selected environment' in str(error)
                return str(error)
            raise AssertionError('Missing package accepted')
    check('missing_selected_environment_package_is_explicit', missing_package)

    def failed_optimizer():
        class Failure:
            success = False
            message = 'controlled optimizer evaluation limit'
            nfev = 1000
        with patch('scipy.optimize.least_squares', return_value=Failure()):
            value = run(action='fit', components=[{'amplitude': -.4, 'center': 4949., 'sigma': 2.}])
        assert value['measurements']['status'] == 'failed' and value['series'] == []
        return value['measurements']
    check('failed_optimizer_returns_status_without_misleading_model', failed_optimizer)

    if args.real_spectrum:
        def real_sdss():
            if not args.core:
                raise ValueError('--core is required for the real shared-packet regression')
            source = args.real_spectrum.resolve()
            before = hashlib.sha256(source.read_bytes()).hexdigest()
            os.environ['FITSVIEWER_CORE_BINARY'] = str(args.core.resolve())
            worker = fs.Worker(args.output/'real-cache')
            options = dict(defaults, path=str(source), hdu=1, x_column=1, y_column=0, error_column=2,
                           source='real SDSS '+source.name, mc_samples=0)
            try:
                packet = worker.dataset(options)
                wx, fy = packet['x'], packet['values']
                valid = np.isfinite(wx) & np.isfinite(fy)
                idx = np.flatnonzero(valid)
                split = np.flatnonzero((np.diff(idx) > 1) | (np.diff(wx[idx]) > 5*np.median(np.diff(wx[idx]))))+1
                longest = max(np.split(idx, split), key=len)
                assert len(longest) >= 80
                lower, upper = int(.3*len(longest)), int(.7*len(longest))
                a, b = float(wx[longest[lower]]), float(wx[longest[upper]])
                windows = [[float(wx[longest[0]]), float(wx[longest[lower-1]])], [float(wx[longest[upper+1]]), float(wx[longest[-1]])]]
                options.update(region=[a, b], continuum_windows=windows)
                value = sa.analyze(worker, options)
                assert value['measurements']['status'] == 'ok'
                baseline = next(v for v in value['series'] if v['kind'] == 'continuum')
                assert len(baseline['x']) <= 8192 and len(baseline['x']) == len(baseline['y'])
                # Preview arrays may be reduced per valid segment even for a short file.
                # 从记录的基线系数在完整原网格独立重建，不能用显示抽稀点科研积分。
                source_line = (wx >= a) & (wx <= b) & valid
                info = value['measurements']['continuum']
                scaled_x = (wx[source_line]-info['x_origin'])/info['x_scale']
                independent_baseline = sum(coefficient*scaled_x**degree for degree, coefficient in enumerate(info['coefficients_scaled_x']))
                residual = fy[source_line]-independent_baseline
                independent_net = float(np.sum(.5*(residual[:-1]+residual[1:])*np.diff(wx[source_line])))
                assert math.isclose(metric(value, 'net_flux_observed')['value'], independent_net, rel_tol=1e-10, abs_tol=1e-12)
                # Every masked/gap boundary within preview coverage must break the line.
                # 此处只检查绘图预算和断点，绝不将预览数组作为数值真值。
                preview_x = np.asarray(baseline['x']); preview_y = np.asarray(baseline['y'], dtype=float)
                boundaries = np.flatnonzero((np.diff(idx) > 1) | (np.diff(wx[idx]) > 5*np.median(np.diff(wx[idx]))))
                for boundary in boundaries:
                    left, right = wx[idx[boundary]], wx[idx[boundary+1]]
                    separator = (preview_x >= left) & (preview_x <= right) & ~np.isfinite(preview_y)
                    assert np.any(separator), {'unbroken_preview_gap': [float(left), float(right)]}
                smooth = sa.analyze(worker, dict(options, action='smooth', smooth_method='boxcar'))
                assert smooth['series'][0]['kind'] == 'smooth'
                assert before == hashlib.sha256(source.read_bytes()).hexdigest()
                report['real_data_executed'] = True
                return {'path': str(source), 'sha256': before, 'samples': len(wx), 'region': [a, b], 'continuum_windows': windows,
                        'independent_net_integral': independent_net, 'preview_samples': len(preview_x), 'preview_gap_checks': len(boundaries),
                        'measurements': value['measurements'], 'versions': value['versions'],
                        'note': 'Real SDSS read-only regression, no asserted line identification or redshift truth.'}
            finally:
                worker.close()
        check('real_SDSS_shared_core_native_integral_regression_and_readonly_hash', real_sdss)
    else:
        report['real_data_note'] = 'Not run: provide --core and --real-spectrum. Controlled analytic truth checks do not replace real SDSS regression.'
        (args.output/'spectral-analysis-validation.json').write_text(json.dumps(fs.clean(report), ensure_ascii=False, indent=2, allow_nan=False), encoding='utf-8')
    passed = sum(row['passed'] for row in report['checks'])
    print(json.dumps({'passed': passed, 'checks': len(report['checks']), 'real_data_executed': report['real_data_executed'], 'output': str(args.output)}))
    return 0 if all(row['passed'] for row in report['checks']) else 1


if __name__ == '__main__':
    raise SystemExit(main())
