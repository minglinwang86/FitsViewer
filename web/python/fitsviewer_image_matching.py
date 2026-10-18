"""Opt-in field-source relative image scaling, never absolute calibration.

仅在所选环境按需计算场源相对计数比例；不重写像素，也不改变原始误差。
"""
import hashlib
import importlib.metadata
import math
from pathlib import Path

from fitsviewer_science import clean, require, bounded_path

MAX_PIXELS = 16_777_216
TILE = 512


def _number(value, name, low, high):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value) or not low <= value <= high:
        raise ValueError(f'{name} must be finite in [{low}, {high}]')
    return float(value)


def _integer(value, name, low, high):
    if isinstance(value, bool) or not isinstance(value, int) or not low <= value <= high:
        raise ValueError(f'{name} must be an integer in [{low}, {high}]')
    return value


def _params(params):
    if not isinstance(params, dict):
        raise ValueError('Expected an image matching parameter object')
    p = dict(params)
    for key in ('path', 'reference_path'):
        if not isinstance(p.get(key), str) or not p[key] or len(p[key]) > 32768:
            raise ValueError(key+' must be a bounded nonempty path')
    for key in ('hdu', 'reference_hdu'):
        if p.get(key) is not None:
            _integer(p[key], key, 0, 1_000_000)
    p.setdefault('source', ''); p.setdefault('revision', 0)
    if not isinstance(p['source'], str) or len(p['source']) > 16384:
        raise ValueError('source must be a bounded string')
    if isinstance(p['revision'], bool) or not isinstance(p['revision'], (str, int)) or (isinstance(p['revision'], str) and len(p['revision']) > 1024):
        raise ValueError('revision must be an integer or bounded string')
    p.setdefault('mode', 'wcs')
    if p['mode'] not in ('wcs', 'pixel'):
        raise ValueError('mode must be wcs or pixel')
    for key in ('assume_aligned', 'allow_different_filters'):
        p.setdefault(key, False)
        if not isinstance(p[key], bool):
            raise ValueError(key+' must be boolean')
    if p['mode'] == 'pixel' and not p['assume_aligned']:
        raise ValueError('Pixel matching requires explicit assume_aligned=true for already registered images')
    for key, default, low, high in [('aperture_radius', 5., 1., 64.), ('annulus_inner', 8., 1., 127.),
            ('annulus_outer', 12., 2., 128.), ('fwhm', 3., 1., 30.),
            ('threshold_sigma', 5., 3., 30.), ('match_radius', 2., .1, 30.)]:
        p[key] = _number(p.get(key, default), key, low, high)
    if not p['aperture_radius'] < p['annulus_inner'] < p['annulus_outer']:
        raise ValueError('Require aperture_radius < annulus_inner < annulus_outer')
    p['max_stars'] = _integer(p.get('max_stars', 200), 'max_stars', 10, 2000)
    p['min_matches'] = _integer(p.get('min_matches', 5), 'min_matches', 3, p['max_stars'])
    for key in ('saturation', 'reference_saturation'):
        p[key] = None if p.get(key) is None else _number(p[key], key, -1e300, 1e300)
    p.update(snr_min=10., clip_sigma=3., maximum_log_scatter=.2, geometry_tolerance_pixels=.5,
             max_total_pixels=MAX_PIXELS, aperture_geometry='pixel-center circular aperture; no fractional-pixel correction')
    return p


def _identity(path, worker):
    path = bounded_path(path)
    stat = path.stat()
    result = {'path': str(path), 'size_bytes': stat.st_size, 'mtime_ns': stat.st_mtime_ns, 'sha256': None,
              'hash_status': 'stat_only_above_32MiB'}
    if stat.st_size <= 32*1024*1024:
        digest = hashlib.sha256()
        with path.open('rb') as stream:
            while True:
                worker.check_cancelled()
                block = stream.read(1024*1024)
                if not block:
                    break
                digest.update(block)
        result.update(sha256=digest.hexdigest(), hash_status='sha256_and_stat')
    _unchanged(result)
    return result


def _unchanged(identity):
    stat = Path(identity['path']).stat()
    if (stat.st_size, stat.st_mtime_ns) != (identity['size_bytes'], identity['mtime_ns']):
        raise ValueError('Source image changed during matching; discard the result and retry')


def _filter(header):
    values = []
    for key in ('FILTER', 'FILTER1', 'FILTER2', 'FILTNAME', 'FILTERID'):
        value = ' '.join(str(header.get(key, '')).lower().split())
        if value and value not in ('open', 'clear', 'none', 'unknown', 'n/a'):
            values.append(value)
    return sorted(set(values))


def _header_info(header):
    return {key: header.get(key) for key in ('FILTER', 'FILTER1', 'FILTER2', 'FILTNAME', 'FILTERID', 'BUNIT',
            'EXPTIME', 'EXPOSURE', 'INSTRUME', 'TELESCOP', 'OBJECT', 'DATE-OBS', 'SATURATE', 'DATAMAX', 'MAXADU') if key in header}


def _saturation(header, explicit):
    if explicit is not None:
        return explicit, 'explicit'
    # DATAMAX is a data range, not a detector linearity threshold.
    # DATAMAX 不能冒充仪器饱和阈值；仅采用明确 SATURATE/MAXADU。
    for key in ('SATURATE', 'MAXADU'):
        value = header.get(key)
        if isinstance(value, (float, int)) and math.isfinite(value):
            return float(value), key
    return None, 'unknown'


def _geometry(target_header, reference_header, target_shape, p, worker, np):
    if p['mode'] == 'pixel':
        return {'status': 'ok', 'matrix': [[1., 0., 0.], [0., 1., 0.]], 'max_residual_pixels': None,
                'rms_residual_pixels': None, 'sample_count': 0, 'coordinate_origin': 0,
                'model': 'identity explicitly assumed by user; not a solved registration'}, None
    from astropy.wcs import WCS
    from astropy.wcs.utils import pixel_to_skycoord, skycoord_to_pixel
    try:
        target = WCS(target_header, naxis=2).celestial
        reference = WCS(reference_header, naxis=2).celestial
        if not target.has_celestial or not reference.has_celestial:
            raise ValueError('Both images require celestial WCS for sky matching')
        height, width = target_shape
        yy, xx = np.meshgrid(np.linspace(0., height-1., 5), np.linspace(0., width-1., 5), indexing='ij')
        points = np.column_stack((xx.ravel(), yy.ravel()))
        worker.check_cancelled()
        sky = pixel_to_skycoord(points[:, 0], points[:, 1], target, origin=0, mode='all')
        rx, ry = skycoord_to_pixel(sky, reference, origin=0, mode='all')
        projected = np.column_stack((rx, ry))
        if not np.isfinite(projected).all():
            raise ValueError('WCS mapping is nonfinite within the target footprint')
        center = (np.asarray([width, height])-1.)/2.
        span = np.maximum(center, 1.)
        design = np.column_stack(((points-center)/span, np.ones(len(points))))
        coefficients, _, rank, _ = np.linalg.lstsq(design, projected, rcond=None)
        if rank != 3:
            raise ValueError('Degenerate WCS affine approximation')
        linear = coefficients[:2, :].T/span[None, :]
        offset = coefficients[2, :]-linear@center
        matrix = np.column_stack((linear, offset))
        residual = np.linalg.norm(design@coefficients-projected, axis=1)
        determinant = float(np.linalg.det(linear))
        if not math.isfinite(determinant) or abs(determinant) < 1e-12:
            raise ValueError('WCS mapping has degenerate scale')
        maximum = float(residual.max())
        mapping = {'status': 'ok' if maximum <= p['geometry_tolerance_pixels'] else 'approximate_only',
                   'matrix': matrix.tolist(), 'max_residual_pixels': maximum, 'rms_residual_pixels': float(np.sqrt(np.mean(residual**2))),
                   'sample_count': len(points), 'coordinate_origin': 0, 'determinant': determinant,
                   'model': 'target pixel -> reference pixel; affine approximation to 5x5 WCS footprint grid',
                   'tolerance_pixels': p['geometry_tolerance_pixels'], 'distortion_resampled': False}
        return mapping, (target, reference)
    except (ValueError, TypeError, RuntimeError) as error:
        return {'status': 'unavailable', 'matrix': None, 'reason': str(error), 'coordinate_origin': 0}, None


def _statistics(image, np):
    from astropy.stats import sigma_clipped_stats
    stride = max(1, math.ceil(image.size/250_000))
    sample = image.reshape(-1)[::stride]
    sample = sample[np.isfinite(sample)]
    if len(sample) < 64:
        return None, None
    _, median, noise = sigma_clipped_stats(sample, sigma=3., maxiters=5)
    if not np.isfinite([median, noise]).all() or noise <= 0:
        return None, None
    return float(median), float(noise)


def _sources(image, p, saturation, worker, np):
    """Bounded tiled detection, then native-pixel aperture flux and quality cuts."""
    import heapq
    from scipy.ndimage import gaussian_filter, maximum_filter
    from scipy.spatial import cKDTree
    from astropy.stats import sigma_clipped_stats
    background, noise = _statistics(image, np)
    rejected = {}

    def reject(reason):
        rejected[reason] = rejected.get(reason, 0)+1

    if background is None:
        return [], {'background': None, 'background_sigma': None, 'detected': 0, 'accepted': 0,
                    'rejected': {'unusable_background_noise': 1}}
    sigma = p['fwhm']/2.354820045
    halo = math.ceil(4*sigma)+math.ceil(p['fwhm'])+2
    size = 2*math.ceil(p['fwhm'])+1
    grid = np.arange(-math.ceil(4*sigma), math.ceil(4*sigma)+1)
    kernel = np.exp(-.5*(grid/sigma)**2); kernel /= kernel.sum()
    filtered_noise = noise*float(np.sum(kernel*kernel))
    candidate_limit = min(10_000, p['max_stars']*8, int(20_000_000/(2*math.ceil(p['annulus_outer'])+3)**2))
    heap = []
    for y0 in range(0, image.shape[0], TILE):
        for x0 in range(0, image.shape[1], TILE):
            worker.check_cancelled()
            y1, x1 = min(y0+TILE, image.shape[0]), min(x0+TILE, image.shape[1])
            by, bx = max(0, y0-halo), max(0, x0-halo)
            block = image[by:min(y1+halo, image.shape[0]), bx:min(x1+halo, image.shape[1])]
            finite = np.isfinite(block)
            smooth = gaussian_filter(np.where(finite, block-background, 0.), sigma, mode='constant', cval=0.)
            peak = (smooth == maximum_filter(smooth, size=size, mode='constant', cval=0.)) & (smooth > p['threshold_sigma']*filtered_noise) & finite
            cut = peak[y0-by:y1-by, x0-bx:x1-bx]
            yy, xx = np.nonzero(cut)
            scores = smooth[yy+y0-by, xx+x0-bx]
            order = np.argsort(scores)[-candidate_limit:]
            for index in order:
                candidate = (float(scores[index]), int(xx[index]+x0), int(yy[index]+y0))
                if len(heap) < candidate_limit:
                    heapq.heappush(heap, candidate)
                elif candidate[0] > heap[0][0]:
                    heapq.heapreplace(heap, candidate)
    candidates = sorted(heap, reverse=True)
    centers = []
    radius = max(2, math.ceil(1.5*p['fwhm']))
    for score, x, y in candidates:
        worker.check_cancelled()
        if x-radius < 0 or y-radius < 0 or x+radius >= image.shape[1] or y+radius >= image.shape[0]:
            reject('edge'); continue
        block = image[y-radius:y+radius+1, x-radius:x+radius+1]
        if not np.isfinite(block).all():
            reject('bad_pixels'); continue
        weight = np.maximum(block-background, 0.)
        total = weight.sum()
        if not math.isfinite(total) or total <= 0:
            reject('nonpositive_flux'); continue
        gy, gx = np.mgrid[y-radius:y+radius+1, x-radius:x+radius+1]
        cx, cy = float(np.sum(gx*weight)/total), float(np.sum(gy*weight)/total)
        vx, vy = float(np.sum((gx-cx)**2*weight)/total), float(np.sum((gy-cy)**2*weight)/total)
        width = 2.354820045*math.sqrt(max(0., (vx+vy)/2.))
        roundness = abs(vx-vy)/max(vx+vy, np.finfo(float).tiny)
        if not .5*p['fwhm'] <= width <= 2.*p['fwhm'] or roundness > .5:
            reject('nonstellar_shape'); continue
        centers.append((cx, cy, score, width))
    if not centers:
        return [], {'background': background, 'background_sigma': noise, 'detected': len(candidates), 'accepted': 0, 'rejected': rejected}
    xy = np.asarray([row[:2] for row in centers])
    neighbors = cKDTree(xy).query(xy, k=2)[0][:, 1] if len(xy) > 1 else np.full(1, np.inf)
    stars = []
    outer = p['annulus_outer']
    for index, (cx, cy, score, width) in enumerate(centers):
        worker.check_cancelled()
        if neighbors[index] < 2*outer:
            reject('crowded_nonindependent'); continue
        left, right = math.floor(cx-outer), math.ceil(cx+outer)+1
        bottom, top = math.floor(cy-outer), math.ceil(cy+outer)+1
        if min(left, bottom) < 0 or right > image.shape[1] or top > image.shape[0]:
            reject('edge'); continue
        block = image[bottom:top, left:right]
        gy, gx = np.mgrid[bottom:top, left:right]
        distance2 = (gx-cx)**2+(gy-cy)**2
        aperture = distance2 <= p['aperture_radius']**2
        annulus = (distance2 >= p['annulus_inner']**2) & (distance2 <= outer**2)
        relevant = aperture | annulus
        if np.count_nonzero(aperture) < 4 or np.count_nonzero(annulus) < 16 or not np.isfinite(block[relevant]).all():
            reject('bad_pixels_or_small_annulus'); continue
        peak_value = float(np.max(block[aperture]))
        if saturation is not None and peak_value >= saturation:
            reject('saturated'); continue
        if np.count_nonzero(block[aperture] == peak_value) >= 3:
            reject('flat_peak_possible_saturation'); continue
        _, local_background, local_sigma = sigma_clipped_stats(block[annulus], sigma=3., maxiters=5)
        if not np.isfinite([local_background, local_sigma]).all() or local_sigma <= 0:
            reject('unusable_local_noise'); continue
        count = int(np.count_nonzero(aperture)); ring_count = int(np.count_nonzero(annulus))
        flux = float(np.sum(block[aperture]-local_background))
        # A screening S/N using annular scatter; no gain/read-noise or formal error claim.
        # 环背景散布仅作筛选SNR，未建模gain/Poisson/像素相关，不作正式测光误差。
        screening_sigma = float(local_sigma)*math.sqrt(count+count*count/ring_count)
        snr = flux/screening_sigma
        if not math.isfinite(flux) or flux <= 0 or not math.isfinite(snr) or snr < p['snr_min']:
            reject('low_SNR_or_nonpositive_flux'); continue
        stars.append({'x': cx, 'y': cy, 'flux': flux, 'screening_snr': snr,
                      'background': float(local_background), 'background_sigma': float(local_sigma),
                      'aperture_pixels': count, 'annulus_pixels': ring_count, 'width_moment_pixels': width})
        if len(stars) >= p['max_stars']:
            break
    return stars, {'background': background, 'background_sigma': noise, 'detected': len(candidates), 'accepted': len(stars),
                   'candidate_limit': candidate_limit, 'candidate_limit_reached': len(candidates) >= candidate_limit, 'rejected': rejected}


def _pairs(target, reference, wcs, p, worker, np):
    if not target or not reference:
        return [], {'unmatched_or_ambiguous': len(target)}
    txy = np.asarray([[s['x'], s['y']] for s in target]); rxy = np.asarray([[s['x'], s['y']] for s in reference])
    worker.check_cancelled()
    if p['mode'] == 'wcs':
        from astropy.wcs.utils import pixel_to_skycoord
        from astropy import units as u
        ts = pixel_to_skycoord(txy[:, 0], txy[:, 1], wcs[0], origin=0, mode='all')
        rs = pixel_to_skycoord(rxy[:, 0], rxy[:, 1], wcs[1], origin=0, mode='all')
        forward, separation, _ = ts.match_to_catalog_sky(rs)
        reverse, reverse_sep, _ = rs.match_to_catalog_sky(ts)
        distance = separation.to_value(u.arcsec)
        td2 = ts.match_to_catalog_sky(rs, nthneighbor=2)[1].to_value(u.arcsec) if len(rs) > 1 else np.full(len(ts), np.inf)
        rd2 = rs.match_to_catalog_sky(ts, nthneighbor=2)[1].to_value(u.arcsec) if len(ts) > 1 else np.full(len(rs), np.inf)
        unit = 'arcsec'
    else:
        from scipy.spatial import cKDTree
        td, ti = cKDTree(rxy).query(txy, k=2)
        rd, ri = cKDTree(txy).query(rxy, k=2)
        forward, reverse, distance, td2, rd2 = ti[:, 0], ri[:, 0], td[:, 0], td[:, 1], rd[:, 1]
        unit = 'pixel'
    pairs = []
    rejected = 0
    for i, j in enumerate(forward):
        worker.check_cancelled()
        j = int(j)
        if distance[i] > p['match_radius'] or reverse[j] != i or td2[i] <= p['match_radius'] or rd2[j] <= p['match_radius']:
            rejected += 1; continue
        a, b = target[i], reference[j]
        ratio = b['flux']/a['flux']
        if not math.isfinite(ratio) or ratio <= 0:
            rejected += 1; continue
        pairs.append({'target': [a['x'], a['y']], 'reference': [b['x'], b['y']],
                      'target_flux': a['flux'], 'reference_flux': b['flux'], 'ratio': ratio,
                      'snr_target': a['screening_snr'], 'snr_reference': b['screening_snr'],
                      'separation': float(distance[i]), 'separation_unit': unit, 'used': True, 'reason': ''})
    return pairs, {'unmatched_or_ambiguous': rejected}


def match_flux(worker, params):
    """Return a robust multiplier; failure always leaves factor=None."""
    p = _params(params)
    worker.check_cancelled()
    require('numpy', 'scipy', 'astropy')
    import numpy as np
    reference_params = {'path': p['reference_path'], 'hdu': p.get('reference_hdu')}
    if p.get('reference_wcs_override') is not None:
        reference_params['wcs_override'] = p['reference_wcs_override']
    tpath, thdu, th, _ = worker.image(p, data=False)
    rpath, rhdu, rh, _ = worker.image(reference_params, data=False)
    expected_pixels = sum(int(h.get('NAXIS1', 0))*int(h.get('NAXIS2', 0)) for h in (th, rh))
    if expected_pixels > MAX_PIXELS or expected_pixels <= 0:
        raise ValueError('Combined image size must be at most 16777216 native pixels')
    identities = [_identity(tpath, worker), _identity(rpath, worker)]
    _, _, _, target = worker.image(dict(p, hdu=thdu), data=True)
    worker.check_cancelled()
    _, _, _, reference = worker.image(dict(reference_params, hdu=rhdu), data=True)
    target = np.asarray(target, dtype=float); reference = np.asarray(reference, dtype=float)
    if target.ndim != 2 or reference.ndim != 2 or target.size+reference.size > MAX_PIXELS:
        raise ValueError('Expected two supported 2D images within the combined pixel cap')
    warnings = ['Relative native-value aperture flux ratio for display; no catalogue/standard stars, absolute zeropoint or physical calibration.',
                'EXPTIME is recorded only; no additional exposure-time multiplication is applied.',
                'Annulus-scatter SNR is a screening statistic, not a formal Poisson/gain/correlated-noise uncertainty.',
                'Different seeing, aperture losses, color terms, variable sources and background structure can bias this relative scale.']
    if p['mode'] == 'pixel':
        warnings.append('Pixel coordinates are explicitly assumed registered by the user; this operation does not solve an image shift.')
    mapping, wcs = _geometry(th, rh, target.shape, p, worker, np)
    result = {'operation': 'image_flux_match', 'source': p['source'], 'revision': p['revision'],
              'path': str(Path(tpath).resolve()), 'hdu': thdu, 'reference_path': str(Path(rpath).resolve()), 'reference_hdu': rhdu,
              'status': 'insufficient_matches', 'factor': None, 'factor_uncertainty': None,
              'matched': 0, 'used': 0, 'rejected': 0, 'scatter_log': None, 'scatter_fraction': None,
              'matches': [], 'rejection_counts': {}, 'mapping': mapping, 'parameters': p, 'warnings': warnings,
              'provenance': {'target': identities[0], 'reference': identities[1], 'target_header': _header_info(th),
                  'reference_header': _header_info(rh), 'target_shape': list(target.shape), 'reference_shape': list(reference.shape),
                  'coordinate_origin': 0, 'source_pixels_modified': False, 'absolute_calibration': False,
                  'data': 'native pixels from worker.image/shared core when configured',
                  'matching': 'mutual one-to-one nearest, excluding all second neighbors within matching radius',
                  'factor_definition': 'multiply target native pixel values to match reference native scale; background offsets are not applied',
                  'noise_model': 'annular background screening only; no gain/read-noise/Poisson model or formal factor error',
                  'algorithm': 'tiled Gaussian-filtered peaks; shape/crowding/badpixel/saturation cuts; circular aperture minus clipped annular median; median clipped log-ratio'},
              'versions': {name: importlib.metadata.version(name) for name in ('numpy', 'scipy', 'astropy')}}

    def finish():
        worker.check_cancelled()
        for identity in identities:
            _unchanged(identity)
        return clean(result)

    if p['mode'] == 'wcs' and wcs is None:
        result.update(status='geometry_unavailable')
        return finish()
    tf, rf = _filter(th), _filter(rh)
    result['provenance'].update(target_filter_tokens=tf, reference_filter_tokens=rf)
    if (not tf or not rf) and not p['allow_different_filters']:
        result.update(status='filter_unverified'); return finish()
    if tf != rf and not p['allow_different_filters']:
        result.update(status='filter_mismatch'); return finish()
    if not tf or not rf or tf != rf:
        warnings.append('Unknown/different filter override explicitly enabled: this is only an approximate display scale, not cross-band calibration.')
    if th.get('INSTRUME') != rh.get('INSTRUME') or th.get('TELESCOP') != rh.get('TELESCOP'):
        warnings.append('Instrument/telescope metadata differ; identical filter labels do not establish identical throughput or a color-free photometric transformation.')
    tsat, tmethod = _saturation(th, p['saturation']); rsat, rmethod = _saturation(rh, p['reference_saturation'])
    result['provenance'].update(target_saturation={'value': tsat, 'source': tmethod}, reference_saturation={'value': rsat, 'source': rmethod})
    if tsat is None or rsat is None:
        warnings.append('Detector saturation/linearity threshold is unknown for at least one image; flat peaks are rejected conservatively but nonlinear sources cannot be guaranteed absent.')
    target_stars, target_detection = _sources(target, p, tsat, worker, np)
    reference_stars, reference_detection = _sources(reference, p, rsat, worker, np)
    result['detection'] = {'target': target_detection, 'reference': reference_detection}
    pairs, rejected = _pairs(target_stars, reference_stars, wcs, p, worker, np)
    result.update(matches=pairs, matched=len(pairs), rejection_counts={'matching': rejected,
                  'target_detection': target_detection['rejected'], 'reference_detection': reference_detection['rejected']})
    if len(pairs) < p['min_matches']:
        result.update(rejected=len(pairs), reason='Too few isolated, valid, high-screening-SNR one-to-one sources; no scale is applied.')
        return finish()
    log_ratios = np.log(np.asarray([row['ratio'] for row in pairs]))
    keep = np.ones(len(pairs), dtype=bool)
    for _ in range(5):
        worker.check_cancelled()
        middle = float(np.median(log_ratios[keep]))
        sigma = float(1.4826022185*np.median(np.abs(log_ratios[keep]-middle)))
        new = np.abs(log_ratios-middle) <= max(p['clip_sigma']*sigma, 1e-6)
        if np.array_equal(new, keep):
            break
        keep = new
        if np.count_nonzero(keep) < p['min_matches']:
            break
    used = int(np.count_nonzero(keep))
    for row, accepted in zip(pairs, keep):
        row['used'] = bool(accepted); row['reason'] = '' if accepted else 'log_ratio_outlier'
    result.update(used=used, rejected=len(pairs)-used)
    if used < p['min_matches']:
        result.update(reason='Too few matches remain after robust log-ratio clipping.'); return finish()
    middle = float(np.median(log_ratios[keep]))
    sigma = float(1.4826022185*np.median(np.abs(log_ratios[keep]-middle)))
    result.update(scatter_log=sigma, scatter_fraction=float(math.expm1(sigma)))
    if sigma > p['maximum_log_scatter']:
        result.update(reason='Matched flux ratios are too inconsistent for a single relative multiplier.'); return finish()
    factor = math.exp(middle)
    if not math.isfinite(factor) or not 1e-12 <= factor <= 1e12:
        result.update(reason='Inferred multiplier is outside the safe display range.'); return finish()
    result.update(status='ok', factor=factor)
    if mapping['status'] == 'approximate_only':
        warnings.append('WCS is not well represented by one affine transform across the image; relative flux matching does not establish pixel registration.')
    return finish()
