"""Read-only, opt-in 1D analysis in the selected scientific environment.

科研计算使用完整 core packet；仅返回绘图序列时降采样，不修改原谱。
All intervals and profile widths use the source x values interpreted in `unit`.
"""
import importlib.metadata
import hashlib
import math
from pathlib import Path

from fitsviewer_science import clean, require, bounded_path

MAX_SAMPLES = 1_000_000
MAX_PREVIEW = 8192


def _fingerprint(worker, params):
    """Identify the file separately from the UI source/revision ownership token."""
    value = params.get('path')
    if value is None:
        return {'status': 'unavailable', 'reason': 'No on-disk path was supplied (e.g. an in-memory test fixture)'}
    if not isinstance(value, str):
        raise ValueError('path must be a string')
    path = bounded_path(value)
    before = path.stat()
    identity = {'path': str(path), 'size_bytes': before.st_size, 'mtime_ns': before.st_mtime_ns,
                'sha256': None, 'status': 'stat_only', 'hash_policy': 'stream SHA256 up to 32 MiB; stat identity for larger files'}
    if before.st_size <= 32*1024*1024:
        digest = hashlib.sha256()
        with path.open('rb') as stream:
            while True:
                worker.check_cancelled()
                block = stream.read(1024*1024)
                if not block:
                    break
                digest.update(block)
        identity.update(sha256=digest.hexdigest(), status='sha256_and_stat')
    after = path.stat()
    if (before.st_size, before.st_mtime_ns) != (after.st_size, after.st_mtime_ns):
        raise ValueError('Source file changed while its identity was being read; retry analysis')
    return identity


def _number(value, name, lower=None, upper=None):
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(name + ' must be a finite number')
    value = float(value)
    if not math.isfinite(value) or (lower is not None and value < lower) or (upper is not None and value > upper):
        raise ValueError(name + ' is outside the supported range')
    return value


def _integer(value, name, lower, upper):
    if isinstance(value, bool) or not isinstance(value, int) or not lower <= value <= upper:
        raise ValueError(f'{name} must be an integer in [{lower}, {upper}]')
    return value


def _choice(value, name, choices):
    if value not in choices:
        raise ValueError(name + ' must be one of ' + ', '.join(choices))
    return value


def _boolean(value, name):
    if not isinstance(value, bool):
        raise ValueError(name + ' must be boolean')
    return value


def _interval(value, name):
    if not isinstance(value, (list, tuple)) or len(value) != 2:
        raise ValueError(name + ' must contain [low, high]')
    result = [_number(v, name) for v in value]
    if result[0] >= result[1]:
        raise ValueError(name + ' must have low < high')
    return result


def _intervals(value, name):
    if not isinstance(value, (list, tuple)) or len(value) > 256:
        raise ValueError(name + ' must contain at most 256 intervals')
    return [_interval(v, name) for v in value]


def _parameters(params):
    if not isinstance(params, dict):
        raise ValueError('Expected a parameter object')
    p = dict(params)
    p['action'] = _choice(p.get('action', 'measure'), 'action', ('continuum', 'measure', 'smooth', 'fit'))
    p['unit'] = _choice(p.get('unit'), 'unit', ('Angstrom', 'nm', 'um'))
    p['air'] = _boolean(p.get('air', False), 'air')
    p['redshift'] = _number(p.get('redshift', 0.), 'redshift', -1., 20.)
    if p['redshift'] <= -1:
        raise ValueError('redshift must be greater than -1')
    if not isinstance(p.get('source', ''), str) or len(p.get('source', '')) > 16384:
        raise ValueError('source must be a bounded string')
    p.setdefault('source', '')
    revision = p.get('revision', 0)
    if isinstance(revision, bool) or not isinstance(revision, (str, int)) or (isinstance(revision, str) and len(revision) > 1024):
        raise ValueError('revision must be an integer or bounded string')
    p['revision'] = revision
    for key in ('hdu', 'error_hdu', 'x_column', 'y_column', 'error_column'):
        if p.get(key) is not None:
            _integer(p[key], key, 0, 4095 if key in ('hdu', 'error_hdu') else 255)
    if p.get('error_kind') is not None:
        _choice(p['error_kind'], 'error_kind', ('auto', 'sigma', 'variance', 'ivar'))
    if 'no_error' in p:
        _boolean(p['no_error'], 'no_error')
    p['region'] = _interval(p['region'], 'region') if p.get('region') is not None else None
    if p['action'] in ('measure', 'fit') and p['region'] is None:
        raise ValueError('Select an explicit region before measuring or fitting')
    p['continuum_windows'] = _intervals(p.get('continuum_windows', []), 'continuum_windows')
    p['masks'] = _intervals(p.get('masks', []), 'masks')
    p['continuum_method'] = _choice(p.get('continuum_method', 'linear'), 'continuum_method', ('linear', 'poly'))
    degree = _integer(p.get('continuum_degree', 1), 'continuum_degree', 0, 5)
    p['continuum_degree'] = 1 if p['continuum_method'] == 'linear' else degree
    p['continuum_output'] = _choice(p.get('continuum_output', 'all'), 'continuum_output', ('all', 'continuum', 'normalized', 'subtracted'))
    p['smooth_method'] = _choice(p.get('smooth_method', 'boxcar'), 'smooth_method', ('boxcar', 'gaussian', 'savgol'))
    p['smooth_window'] = _integer(p.get('smooth_window', 7), 'smooth_window', 3, 1001)
    if not p['smooth_window'] % 2:
        raise ValueError('smooth_window must be odd')
    p['smooth_sigma'] = _number(p.get('smooth_sigma', 2.), 'smooth_sigma', .1, 333.)
    p['smooth_polyorder'] = _integer(p.get('smooth_polyorder', 2), 'smooth_polyorder', 0, 5)
    if p['smooth_polyorder'] >= p['smooth_window']:
        raise ValueError('smooth_polyorder must be smaller than smooth_window')
    p['mc_samples'] = _integer(p.get('mc_samples', 128), 'mc_samples', 0, 512)
    p['seed'] = _integer(p.get('seed', 20261017), 'seed', 0, 2**32-1)
    p['fit_profile'] = _choice(p.get('fit_profile', 'gaussian'), 'fit_profile', ('gaussian', 'voigt'))
    p['fit_include_continuum'] = _boolean(p.get('fit_include_continuum', True), 'fit_include_continuum')
    p['gap_factor'] = 5.  # Fixed conservative heuristic, recorded rather than hidden.
    return p


class _Data:
    def __init__(self, worker, p, np):
        self.worker, self.p, self.np = worker, p, np
        self.warnings = []
        worker.check_cancelled()
        packet = worker.dataset(p)
        meta = packet.get('meta', {}).get('spectrum')
        if not isinstance(meta, dict) or 'x' not in packet:
            raise ValueError('Open a one-dimensional spectrum first')
        label = str(meta.get('x_label', '')).lower()
        source_unit = str(meta.get('x_unit', '')).lower().strip()
        if any(v in label for v in ('sample', 'pixel', 'index', 'frequency', 'velocity', 'energy')) or source_unit.endswith('hz') or source_unit in ('pixel', 'pix', 'index', 's', 'sec', 'km/s', 'm/s', 'ev', 'kev'):
            raise ValueError('Analysis requires a wavelength axis with an explicit supported unit')
        # These source arrays remain read-only; derived arrays and MC trials own memory.
        # 源数组只读，派生结果与 MC 样本单独分配，避免多保留一份完整源谱。
        self.x = np.asarray(packet['x'], dtype=float)
        self.y = np.asarray(packet['values'], dtype=float)
        if self.x.ndim != 1 or self.y.shape != self.x.shape or not 5 <= len(self.x) <= MAX_SAMPLES:
            raise ValueError('Expected 5 to 1000000 paired source samples')
        finite_x = np.isfinite(self.x)
        if np.any(self.x[finite_x] <= 0) or np.count_nonzero(finite_x) < 5:
            raise ValueError('Finite wavelengths must be positive; at least five are required')
        dx = np.diff(self.x[finite_x])
        self.descending = bool(np.all(dx < 0))
        if not (self.descending or np.all(dx > 0)):
            raise ValueError('Wavelengths must be strictly monotonic without duplicates')
        self.error = packet.get('error') if not p.get('no_error', False) else None
        if self.error is not None:
            self.error = np.asarray(self.error, dtype=float)
            if self.error.shape != self.y.shape:
                raise ValueError('Uncertainty shape differs from source spectrum')
        self.original_indices = np.arange(len(self.x))
        if self.descending:
            self.x, self.y, self.original_indices = self.x[::-1], self.y[::-1], self.original_indices[::-1]
            if self.error is not None:
                self.error = self.error[::-1]
        self.valid = np.isfinite(self.x) & np.isfinite(self.y)
        for low, high in p['masks']:
            self.valid &= ~((self.x >= low) & (self.x <= high))
        if np.count_nonzero(self.valid) < 5:
            raise ValueError('Too few unmasked finite samples')
        self.step = float(np.median(np.abs(dx)))
        self.flux_unit = str(meta.get('y_unit') or 'source flux (unit unknown)')
        self.meta = meta
        finite = self.x[np.isfinite(self.x)]
        if p['region'] and (p['region'][0] < finite[0] or p['region'][1] > finite[-1]):
            raise ValueError('region must stay within the source wavelength range')
        if not meta.get('y_unit'):
            self.warnings.append('Flux unit is unknown: integrals retain source flux units; no physical flux calibration is inferred.')
        self.warnings.append('Air/vacuum is a source-axis interpretation only; no wavelength samples are converted or barycentrically corrected.')

    def segments(self, valid=None):
        np = self.np
        indices = np.flatnonzero(self.valid if valid is None else valid)
        if not len(indices):
            return []
        jumps = (np.diff(indices) > 1) | (np.diff(self.x[indices]) > self.p['gap_factor'] * self.step)
        return list(np.split(indices, np.flatnonzero(jumps) + 1))

    def region(self):
        low, high = self.p['region']
        return (self.x >= low) & (self.x <= high)

    def baseline_selection(self):
        np, p = self.np, self.p
        use = self.valid.copy()
        if p['continuum_windows']:
            inside = np.zeros(len(self.x), dtype=bool)
            for low, high in p['continuum_windows']:
                inside |= (self.x >= low) & (self.x <= high)
            use &= inside
        if p['region']:
            use &= ~self.region()
        if np.count_nonzero(use) < p['continuum_degree'] + 2:
            raise ValueError('Too few continuum samples outside the line region; select baseline windows')
        return use

    def baseline(self, y=None, selection=None):
        np = self.np
        use = self.baseline_selection() if selection is None else selection
        x = self.x[use]
        y = self.y if y is None else y
        origin = float((x.min() + x.max()) / 2.)
        span = float((x.max() - x.min()) / 2.)
        if not span > 0:
            raise ValueError('Continuum windows have no wavelength span')
        design = np.polynomial.polynomial.polyvander((x-origin)/span, self.p['continuum_degree'])
        error_ok = self.error is not None and bool(np.all(np.isfinite(self.error[use]) & (self.error[use] > 0)))
        weight = 1./self.error[use] if error_ok else np.ones(len(x))
        # Normalizing weights avoids overflow at astrophysical flux scales.
        # 按中值归一化权重，不改变最小二乘解，避免极小物理单位造成溢出。
        weight /= np.median(weight)
        coefficient, _, rank, singular = np.linalg.lstsq(design * weight[:, None], y[use] * weight, rcond=None)
        if rank != self.p['continuum_degree']+1:
            raise ValueError('Continuum fit is rank deficient; change the windows or degree')
        with np.errstate(over='ignore', invalid='ignore'):
            prediction = np.polynomial.polynomial.polyval((self.x-origin)/span, coefficient)
        prediction[~self.valid] = np.nan
        info = {'method': self.p['continuum_method'], 'degree': self.p['continuum_degree'],
                'samples': int(np.count_nonzero(use)), 'windows': self.p['continuum_windows'],
                'excluded_region': self.p['region'], 'weights': '1/source_sigma' if error_ok else 'unweighted',
                'coefficients_scaled_x': coefficient.tolist(), 'x_origin': origin, 'x_scale': span,
                'rank': int(rank), 'source_sample_span': [float(x.min()), float(x.max())],
                'interpretation': 'local fitted (pseudo-)continuum; line region excluded; no clipping or hidden line rejection'}
        return prediction, info

    def series(self, name, kind, y):
        np = self.np
        values = np.asarray(y, dtype=float)
        if values.shape != self.x.shape:
            raise ValueError('Derived series shape differs from source')
        valid = self.valid & np.isfinite(values)
        segments = self.segments(valid)
        if len(segments) > MAX_PREVIEW//5:
            self.warnings.append(name + ': preview omitted because too many gaps would exceed the safe display limit.')
            return {'name': name, 'kind': kind, 'x': [], 'y': [], 'preview_only': True, 'source_samples': len(self.x)}
        if not segments:
            return {'name': name, 'kind': kind, 'x': [], 'y': [], 'preview_only': True, 'source_samples': len(self.x)}
        budget = max(4, (MAX_PREVIEW-len(segments)+1)//len(segments))
        out_x, out_y = [], []
        for segment in segments:
            self.worker.check_cancelled()
            if len(segment) <= budget:
                selected = segment
            else:
                indices = {int(segment[0]), int(segment[-1])}
                for group in np.array_split(segment, max(1, (budget-2)//2)):
                    indices.add(int(group[np.argmin(values[group])]))
                    indices.add(int(group[np.argmax(values[group])]))
                selected = np.asarray(sorted(indices), dtype=int)
            if out_x:
                out_x.append(float(self.x[segment[0]])); out_y.append(None)
            out_x.extend(self.x[selected].tolist()); out_y.extend(values[selected].tolist())
        if self.descending:
            out_x.reverse(); out_y.reverse()
        return {'name': name, 'kind': kind, 'x': out_x, 'y': out_y, 'preview_only': True,
                'source_samples': len(self.x), 'display_samples': len(out_x), 'sampling': 'native samples; per-segment extrema retained'}


def _continuum(data):
    np = data.np
    continuum, info = data.baseline()
    normalized = np.full(len(data.x), np.nan)
    positive = data.valid & np.isfinite(continuum) & (continuum > 0)
    normalized[positive] = data.y[positive]/continuum[positive]
    if np.any(data.valid & ~positive):
        data.warnings.append('Normalization is undefined where the fitted continuum is nonpositive or nonfinite.')
    series = []
    for name, values in (('continuum', continuum), ('normalized', normalized), ('subtracted', data.y-continuum)):
        if data.p['continuum_output'] in ('all', name):
            series.append(data.series(name, name, values))
    return series, {'status': 'ok', 'continuum': info}


def _metrics(data, y, continuum, indices):
    np, p = data.np, data.p
    x, flux, cont = data.x[indices], y[indices], continuum[indices]
    dx = np.diff(x)
    weights = np.zeros(len(x))
    weights[:-1] += dx/2.; weights[1:] += dx/2.
    residual = flux-cont
    net = float(np.dot(weights, residual))
    polarity = -1 if net < 0 else 1
    profile = polarity*residual
    area = float(np.dot(weights, profile))
    positive_area = float(np.dot(weights, np.maximum(profile, 0)))
    negative_area = float(np.dot(weights, np.maximum(-profile, 0)))
    safe_area = area > np.finfo(float).tiny and negative_area <= .25*positive_area
    center = float(np.dot(weights, profile*x)/area) if safe_area else None
    center_status = 'ok' if center is not None and x[0] <= center <= x[-1] else 'mixed_sign_or_zero_area'
    if center_status != 'ok':
        center = None
    width, width_status = None, 'no_positive_peak'
    peak = int(np.argmax(profile)); half = float(profile[peak]/2.)
    above = profile >= half
    if profile[peak] > 0:
        runs = np.count_nonzero(np.diff(np.r_[False, above, False].astype(int)) == 1)
        if above[0] or above[-1]:
            width_status = 'truncated'
        elif runs != 1:
            width_status = 'multiple_peaks'
        else:
            selected = np.flatnonzero(above)
            left, right = int(selected[0]), int(selected[-1])
            xl = x[left-1] + (half-profile[left-1])/(profile[left]-profile[left-1])*(x[left]-x[left-1])
            xr = x[right] + (half-profile[right])/(profile[right+1]-profile[right])*(x[right+1]-x[right])
            width = float(xr-xl); width_status = 'ok'
    ew = float(np.dot(weights, 1.-flux/cont)) if np.all(np.isfinite(cont) & (cont > 0)) else None
    minimum = int(np.argmin(flux))
    zscale = 1.+p['redshift']
    values = [
        ('equivalent_width_observed', 'pEW / EW observed', ew, p['unit'], 'ok' if ew is not None else 'undefined_continuum'),
        ('equivalent_width_rest', 'pEW / EW rest', None if ew is None else ew/zscale, p['unit'], 'ok' if ew is not None else 'undefined_continuum'),
        ('net_flux_observed', 'Net integrated flux observed', net, data.flux_unit+' * '+p['unit'], 'ok'),
        ('minimum_wavelength_observed', 'Original-sample minimum wavelength observed', float(x[minimum]), p['unit'], 'ok'),
        ('minimum_wavelength_rest', 'Original-sample minimum wavelength rest', float(x[minimum]/zscale), p['unit'], 'ok'),
        ('minimum_flux', 'Original-sample minimum flux', float(flux[minimum]), data.flux_unit, 'ok'),
        ('centroid_observed', 'Continuum-subtracted centroid observed', center, p['unit'], center_status),
        ('centroid_rest', 'Continuum-subtracted centroid rest', None if center is None else center/zscale, p['unit'], center_status),
        ('fwhm_observed', 'Observed profile FWHM', width, p['unit'], width_status),
        ('fwhm_rest', 'Rest-coordinate profile FWHM', None if width is None else width/zscale, p['unit'], width_status),
    ]
    return [{'key': k, 'name': n, 'value': v, 'unit': u, 'status': s,
             'uncertainty': None, 'uncertainty_status': 'not_available'} for k, n, v, u, s in values]


def _measure(data):
    np, p = data.np, data.p
    region = data.region()
    indices = np.flatnonzero(region & data.valid)
    # Incomplete windows are rejected instead of joining masked line wings.
    # 不将掩膜两侧线翼拼接为假完整谱线；科研积分也不跨缺口。
    segments = data.segments(region & data.valid)
    if len(indices) < 3 or len(segments) != 1 or np.any(region & ~data.valid):
        return [], {'status': 'incomplete_region', 'metrics': [], 'region': p['region'], 'samples': len(indices),
                    'reason': 'A measurement needs at least three contiguous valid source samples with no masked/gap samples in its region.'}
    continuum_selection = data.baseline_selection()
    continuum, info = data.baseline(selection=continuum_selection)
    metrics = _metrics(data, data.y, continuum, indices)
    uncertainty = {'method': 'none', 'requested_samples': p['mc_samples'], 'successful_samples': 0,
                   'seed': p['seed'], 'includes_continuum_refit': True, 'includes_covariance': False,
                   'unassessed_systematics': ['continuum/window choice', 'wavelength calibration', 'redshift', 'LSF', 'flux calibration']}
    needed = region | continuum_selection
    sigma_ok = data.error is not None and bool(np.all(np.isfinite(data.error[needed]) & (data.error[needed] > 0)))
    if sigma_ok and p['mc_samples'] >= 2:
        if p['mc_samples'] * int(np.count_nonzero(needed)) > 20_000_000:
            raise ValueError('Monte Carlo exceeds 20 million sample draws; reduce mc_samples or continuum windows')
        rng = np.random.default_rng(p['seed'])
        samples = {row['key']: [] for row in metrics}
        active = np.flatnonzero(needed)
        for _ in range(p['mc_samples']):
            data.worker.check_cancelled()
            perturbed = data.y.copy()
            perturbed[active] += rng.normal(size=len(active))*data.error[active]
            baseline, _ = data.baseline(perturbed, continuum_selection)
            trial = _metrics(data, perturbed, baseline, indices)
            for row in trial:
                if row['status'] == 'ok' and row['value'] is not None and math.isfinite(row['value']):
                    samples[row['key']].append(row['value'])
        minimum_success = max(2, int(math.ceil(.8*p['mc_samples'])))
        for row in metrics:
            n = len(samples[row['key']]); row['uncertainty_samples'] = n
            if row['status'] == 'ok' and n >= minimum_success:
                row['uncertainty'] = float(np.std(samples[row['key']], ddof=1))
                row['uncertainty_status'] = 'independent_pixel_MC'
            else:
                row['uncertainty_status'] = 'insufficient_success'
        uncertainty.update(method='independent_source_sigma_Monte_Carlo', successful_samples=len(samples['net_flux_observed']))
        data.warnings.append('Monte Carlo refits the continuum but assumes independent source pixels; systematic errors and resampling/smoothing covariance are not included.')
    else:
        state = 'disabled' if p['mc_samples'] < 2 else 'not_available'
        for row in metrics:
            row['uncertainty_status'] = state
        data.warnings.append('No formal measurement uncertainty: Monte Carlo is disabled or valid positive source sigma is missing in the line/baseline windows.')
    data.warnings.append('EW absorption is positive. Rest values change the wavelength coordinate only; integrated flux remains an observed-frame quantity.')
    data.warnings.append('Minimum wavelength is an original-sample flux minimum, not an interpolated velocity or a species identification; FWHM includes instrumental broadening.')
    series, _ = _continuum(data)
    return series, {'status': 'ok', 'metrics': metrics, 'region': p['region'], 'samples': len(indices),
                    'sample_range': [float(data.x[indices[0]]), float(data.x[indices[-1]])],
                    'minimum_source_index': int(data.original_indices[indices[int(np.argmin(data.y[indices]))]]),
                    'integration': 'trapezoid on complete native sample span; no boundary extrapolation',
                    'uncertainty': uncertainty, 'continuum': info}


def _smooth(data):
    from scipy.ndimage import convolve1d
    from scipy.signal import savgol_filter
    np, p = data.np, data.p
    result = np.full(len(data.x), np.nan)
    skipped = 0
    segments = data.segments()
    window = p['smooth_window']
    for segment in segments:
        data.worker.check_cancelled()
        if len(segment) < window:
            skipped += 1
            continue
        if p['smooth_method'] in ('gaussian', 'savgol'):
            dx = np.diff(data.x[segment]); middle = np.median(dx)
            if np.max(np.abs(dx-middle))/middle > .01:
                raise ValueError('Gaussian/Savitzky-Golay sample windows require a grid uniform within 1%; use boxcar or explicitly resample first')
        if p['smooth_method'] == 'savgol':
            result[segment] = savgol_filter(data.y[segment], window, p['smooth_polyorder'], mode='interp')
        else:
            grid = np.arange(window)-window//2
            kernel = np.ones(window) if p['smooth_method'] == 'boxcar' else np.exp(-.5*(grid/p['smooth_sigma'])**2)
            kernel /= kernel.sum()
            result[segment] = convolve1d(data.y[segment], kernel, mode='nearest')
    if np.count_nonzero(np.isfinite(result)) < 3:
        raise ValueError('No valid segment is long enough for this smoothing window')
    if skipped:
        data.warnings.append(f'{skipped} short valid segments omitted from smoothing; no gaps were filled.')
    data.warnings.append('Smoothed preview only; adjacent errors become correlated. Formal measurements always use the source packet, not this series. Window/sigma are in samples.')
    if p['smooth_method'] == 'boxcar':
        data.warnings.append('Boxcar weights samples equally; on a nonuniform grid its wavelength width varies.')
    return [data.series('smooth', 'smooth', result)], {'status': 'ok', 'method': p['smooth_method'],
                'window_samples': window, 'sigma_samples': p['smooth_sigma'], 'polyorder': p['smooth_polyorder'],
                'edge_mode': 'interp' if p['smooth_method'] == 'savgol' else 'nearest', 'skipped_segments': skipped,
                'uncertainty_status': 'not_propagated_correlated_preview'}


def _fit(data):
    from scipy.optimize import least_squares
    from scipy.special import voigt_profile
    np, p = data.np, data.p
    entries = p.get('components')
    if not isinstance(entries, list) or not 1 <= len(entries) <= 4 or any(not isinstance(v, dict) for v in entries):
        raise ValueError('Provide 1 to 4 explicit profile component initial guesses')
    region = data.region()
    valid = region & data.valid
    indices = np.flatnonzero(valid)
    keys = ('amplitude', 'center', 'sigma') if p['fit_profile'] == 'gaussian' else ('amplitude', 'center', 'fwhm_g', 'fwhm_l')
    count = len(entries)*len(keys)
    if len(indices) <= count+1:
        raise ValueError('Too few valid fit samples for the requested number of parameters')
    if len(indices) > 200_000:
        raise ValueError('Fit region exceeds 200000 samples; choose a smaller region')
    if p['fit_include_continuum']:
        baseline, baseline_info = data.baseline()
    else:
        baseline = np.zeros(len(data.x)); baseline[~data.valid] = np.nan
        baseline_info = {'method': 'zero', 'interpretation': 'Input explicitly declared continuum-subtracted by fit_include_continuum=false'}
    x, y = data.x[indices], data.y[indices]-baseline[indices]
    sigma_ok = data.error is not None and bool(np.all(np.isfinite(data.error[indices]) & (data.error[indices] > 0)))
    noise = float(np.median(data.error[indices])) if sigma_ok else 0.
    scale = max(float(np.max(np.abs(y))), noise, np.finfo(float).tiny)
    origin, span = p['region'][0], p['region'][1]-p['region'][0]
    scales = np.tile([scale, span]+[span]*(len(keys)-2), len(entries))
    offsets = np.tile([0., origin]+[0.]*(len(keys)-2), len(entries))
    initial, lower, upper, labels, bounds_record = [], [], [], [], []
    for number, entry in enumerate(entries, 1):
        bound_entry = entry.get('bounds', {})
        if not isinstance(bound_entry, dict) or set(bound_entry)-set(keys):
            raise ValueError('Unknown profile bound parameter')
        initial_entry, record = {}, {}
        for key in keys:
            value = _number(entry.get(key), 'component '+key)
            default = [-10*scale, 10*scale] if key == 'amplitude' else (p['region'] if key == 'center' else [.1*data.step, span])
            bounds = _interval(bound_entry.get(key, default), 'bounds '+key)
            if key == 'center' and (bounds[0] < origin or bounds[1] > origin+span):
                raise ValueError('Center bounds must stay inside the fit region')
            if key not in ('center', 'amplitude') and bounds[0] <= 0:
                raise ValueError('Profile width lower bounds must be positive')
            if not bounds[0] < value < bounds[1]:
                raise ValueError('Every profile initial value must lie strictly inside its finite bounds')
            initial.append(value); lower.append(bounds[0]); upper.append(bounds[1]); labels.append(f'component {number}.{key}')
            initial_entry[key] = value; record[key] = bounds
        bounds_record.append({'initial': initial_entry, 'bounds': record})
    start = (np.asarray(initial)-offsets)/scales
    lo, hi = (np.asarray(lower)-offsets)/scales, (np.asarray(upper)-offsets)/scales
    weight = scale/data.error[indices] if sigma_ok else np.ones(len(x))

    def profiles(vector, wave):
        physical = vector*scales+offsets
        result = []
        for index in range(len(entries)):
            values = physical[index*len(keys):(index+1)*len(keys)]
            amp, center = values[:2]
            if p['fit_profile'] == 'gaussian':
                result.append(amp*np.exp(-.5*((wave-center)/values[2])**2))
            else:
                gaussian_sigma = values[2]/math.sqrt(8*math.log(2)); gamma = values[3]/2.
                result.append(amp*voigt_profile(wave-center, gaussian_sigma, gamma)/voigt_profile(0., gaussian_sigma, gamma))
        return result

    def residual(vector):
        data.worker.check_cancelled()
        return (np.sum(profiles(vector, x), axis=0)-y)/scale*weight

    result = least_squares(residual, start, bounds=(lo, hi), method='trf', max_nfev=1000, x_scale='jac')
    data.worker.check_cancelled()
    if not result.success or not np.all(np.isfinite(result.x)):
        return [], {'status': 'failed', 'fit': {'success': False, 'message': str(result.message), 'evaluations': result.nfev}}
    physical = result.x*scales+offsets
    singular = np.linalg.svd(result.jac, compute_uv=False)
    threshold = np.finfo(float).eps*max(result.jac.shape)*singular[0]
    rank = int(np.count_nonzero(singular > threshold))
    covariance, standard_errors = None, None
    if sigma_ok and rank == count:
        _, s, vt = np.linalg.svd(result.jac, full_matrices=False)
        covariance = ((vt.T/(s*s))@vt)*scales[:, None]*scales[None, :]
        standard_errors = np.sqrt(np.maximum(np.diag(covariance), 0))
    touched = [labels[i] for i in range(count) if result.active_mask[i] or min(result.x[i]-lo[i], hi[i]-result.x[i]) <= 1e-5*(hi[i]-lo[i])]
    components = []
    for index in range(len(entries)):
        entry = {key: float(physical[index*len(keys)+j]) for j, key in enumerate(keys)}
        entry['uncertainty'] = None if standard_errors is None else {key: float(standard_errors[index*len(keys)+j]) for j, key in enumerate(keys)}
        entry['bounds'] = bounds_record[index]['bounds']; components.append(entry)
    model_parts = profiles(result.x, data.x)
    domain = data.valid & region
    model = np.sum(model_parts, axis=0)+baseline
    model[~domain] = np.nan
    series = [data.series('model', 'model', model)]
    for i, values in enumerate(model_parts, 1):
        values[~domain] = np.nan
        series.append(data.series(f'component {i}', 'component', values))
    series.append(data.series('residual', 'residual', data.y-model))
    dof = len(indices)-count
    if np.any(region & ~data.valid) or len(data.segments(valid)) > 1:
        data.warnings.append('Fit omits masked/gap samples; model preview keeps gaps and no flux is integrated across them.')
    data.warnings.append('Profile amplitudes are peak heights; Voigt widths are Gaussian/Lorentz FWHM separately, not total FWHM. Component shapes alone are not physical identifications.')
    data.warnings.append('Fit covariance, when available, is local and conditions on the fixed fitted continuum; continuum, LSF, redshift and pixel covariance systematics are excluded.')
    if touched:
        data.warnings.append('Some fit parameters reached or approached bounds; inspect components and residuals before interpreting them.')
    if not sigma_ok:
        data.warnings.append('Fit is unweighted because complete positive source sigma is unavailable; no formal parameter uncertainties are reported.')
    return series, {'status': 'ok', 'components': components, 'continuum': baseline_info,
        'fit': {'success': True, 'message': str(result.message), 'profile': p['fit_profile'], 'weighted': sigma_ok,
                'cost': float(result.cost), 'dof': dof, 'samples': len(indices), 'evaluations': result.nfev,
                'reduced_chi2': float(2*result.cost/dof) if sigma_ok and dof > 0 else None,
                'covariance_rank': rank, 'parameter_count': count, 'covariance': covariance,
                'covariance_parameter_order': labels, 'covariance_status': 'conditional_on_fixed_continuum' if covariance is not None else ('rank_deficient' if rank < count else 'no_source_sigma'),
                'boundary_parameters': touched, 'component_initial_bounds': bounds_record}}


def analyze(worker, params):
    """Analyze native samples; optional packages are loaded only on this request."""
    p = _parameters(params)
    worker.check_cancelled()
    require('numpy', 'scipy')
    import numpy as np
    identity = _fingerprint(worker, p)
    data = _Data(worker, p, np)
    worker.check_cancelled()
    if identity.get('path'):
        after = Path(identity['path']).stat()
        if (identity['size_bytes'], identity['mtime_ns']) != (after.st_size, after.st_mtime_ns):
            raise ValueError('Source file changed during analysis; discard this result and retry')
    series, measurements = {'continuum': _continuum, 'measure': _measure, 'smooth': _smooth, 'fit': _fit}[p['action']](data)
    worker.check_cancelled()
    versions = {}
    for package in ('numpy', 'scipy', 'astropy', 'specutils'):
        try:
            versions[package] = importlib.metadata.version(package)
        except importlib.metadata.PackageNotFoundError:
            versions[package] = None
    return clean({'operation': 'spectral_analysis', 'action': p['action'], 'source': p['source'], 'revision': p['revision'],
                  'series': series, 'measurements': measurements, 'parameters': p, 'versions': versions,
                  'warnings': list(dict.fromkeys(data.warnings)),
                  'provenance': {'data': 'shared core packet, full source samples', 'source_samples': len(data.x),
                      'source_file': identity,
                      'descending_source': data.descending, 'source_axis_unit': data.meta.get('x_unit', ''),
                      'interpreted_axis_unit': p['unit'], 'flux_unit': data.flux_unit,
                      'error_input': 'source standard deviation' if data.error is not None else 'none',
                      'source_uncertainty_kind': data.meta.get('uncertainty_kind', ''),
                      'display_reduction': 'only derived preview arrays; scientific computation uses full source arrays',
                      'algorithm_backend': 'NumPy/SciPy; no embedded packages; no flux unit conversion',
                      'frame': 'source frame unchanged; rest coordinates use user redshift only'}})
