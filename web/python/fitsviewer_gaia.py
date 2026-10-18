"""Gaia field recognition/refinement using the selected external Python environment.
Gaia 星点匹配与 WCS 校准；不包含星表、索引或 Python 科学依赖。
"""
import math
import re
import uuid
from fitsviewer_science import require, bounded_path, MAX_TABLE, clean

GAIA_SERVICE = 'https://gea.esac.esa.int/tap-server/tap'
GAIA_SERVICES = (GAIA_SERVICE, 'https://gaia.ari.uni-heidelberg.de/tap')


def checkpoint(params):
    if params.get('_cancel') and params['_cancel'].is_set():
        raise InterruptedError('Gaia calibration cancelled')


def prior_wcs(header, width, height, params):
    import numpy as np
    import astropy.units as u
    from astropy.coordinates import SkyCoord
    from astropy.wcs import WCS
    from astropy.wcs.utils import pixel_to_skycoord

    prior = WCS(header, naxis=2).celestial
    supplied = params.get('ra_deg') is not None and params.get('dec_deg') is not None
    if supplied or not prior.has_celestial:
        ra, dec = params.get('ra_deg'), params.get('dec_deg')
        if ra is None or dec is None:
            ra = header.get('RA', header.get('OBJCTRA'))
            dec = header.get('DEC', header.get('OBJCTDEC'))
        scale = params.get('scale_arcsec', header.get('PIXSCALE', header.get('SECPIX')))
        if ra is None or dec is None or scale is None:
            return None, None, None
        if isinstance(ra, str) and (':' in ra or 'h' in ra):
            center = SkyCoord(ra, str(dec), unit=(u.hourangle, u.deg), frame='icrs')
        else:
            center = SkyCoord(float(ra) * u.deg, float(dec) * u.deg, frame='icrs')
        scale = float(scale)
        rotation = float(params.get('rotation_deg', header.get('CROTA2', 0.)))
        if not math.isfinite(scale) or not .01 <= scale <= 120 or not math.isfinite(rotation):
            raise ValueError('Provide a finite pixel scale between 0.01 and 120 arcsec/pixel')
        angle = math.radians(rotation)
        prior = WCS(naxis=2)
        prior.wcs.ctype = ['RA---TAN', 'DEC--TAN']
        prior.wcs.radesys = 'ICRS'
        prior.wcs.crval = [center.ra.deg, center.dec.deg]
        prior.wcs.crpix = [(width + 1) / 2, (height + 1) / 2]
        prior.wcs.cd = scale / 3600 * np.array([[-math.cos(angle), math.sin(angle)],
                                              [math.sin(angle), math.cos(angle)]])
    center = pixel_to_skycoord((width - 1) / 2, (height - 1) / 2, prior, origin=0).icrs
    corners = pixel_to_skycoord([0, width - 1, width - 1, 0], [0, 0, height - 1, height - 1], prior, origin=0)
    radius = max(float(center.separation(corners).deg.max()) * 1.25, .001)
    if not math.isfinite(radius) or radius > 1.5:
        raise ValueError('Gaia one-click search supports a field radius up to 1.5 degrees')
    return prior, center, radius


def reference_table(worker, center, radius, params):
    from fitsviewer_network import read_table,file_capability_request
    file_capability=file_capability_request(worker,params)
    if params.get('catalogue'):
        return read_table(bounded_path(params['catalogue'], MAX_TABLE),file_capability=file_capability), 'local Gaia reference table', None
    from fitsviewer_tap import query
    maximum = max(50, min(int(params.get('reference_limit', 3000)), 5000))
    magnitude = float(params.get('magnitude_limit', 20.))
    if not math.isfinite(magnitude) or not 5 <= magnitude <= 22:
        raise ValueError('Gaia magnitude limit must be between 5 and 22')
    # Only sky-center/radius parameters are sent. Images and local tables stay local.
    # 网络请求仅含指向与半径，不上传图像或本地星表。
    adql = (f'SELECT TOP {maximum} source_id,ra,dec,ref_epoch,pmra,pmdec,phot_g_mean_mag '
            f'FROM gaiadr3.gaia_source WHERE 1=CONTAINS(POINT(\'ICRS\',ra,dec),'
            f'CIRCLE(\'ICRS\',{center.ra.deg:.12f},{center.dec.deg:.12f},{radius:.12f})) '
            f'AND phot_g_mean_mag<{magnitude:.8f} ORDER BY phot_g_mean_mag ASC')
    from fitsviewer_network import ServiceError
    failures=[]
    for service in GAIA_SERVICES:
        try:
            artifact = query(worker, {'protocol': 'tap', 'service': service, 'adql': adql,
                                     'max_rows': maximum, 'timeout': params.get('timeout',60),
                                     '_cancel': params.get('_cancel'),'_file_capability':file_capability})
            artifact['prior_service_failures']=failures
            return read_table(artifact['_path'],format='votable',file_capability=file_capability), 'Gaia DR3 / '+service, artifact
        except ServiceError as error:failures.append({'service':service,'error':str(error),'category':error.category})
    raise ServiceError('Gaia services failed: '+'; '.join(item['error'] for item in failures),failures[-1]['category'])


def observation_time(epoch):
    """Read modern and legacy FITS epochs without a two-digit-year pivot.

    旧 FITS DD/MM/YY 固定属于 1900--1999；不能用系统日期解析器猜世纪。
    https://fits.gsfc.nasa.gov/year2000.html section 3.1.
    """
    from astropy.time import Time
    try:
        if isinstance(epoch,bool):raise ValueError()
        if isinstance(epoch,(int,float)):
            observation=Time(float(epoch),format='jyear')
        elif isinstance(epoch,str):
            text=epoch.strip()
            legacy=re.fullmatch(r'(\d{2})/(\d{2})/(\d{2})',text)
            if legacy:
                day,month,year=map(int,legacy.groups())
                text=f'{1900+year:04d}-{month:02d}-{day:02d}'
                observation=Time(text,format='fits')
            elif re.fullmatch(r'[+-]?\d+(?:\.\d+)?',text):
                observation=Time(float(text),format='jyear')
            else:observation=Time(text)
        else:raise ValueError()
        if not observation.isscalar or not math.isfinite(float(observation.jd)):raise ValueError()
        return observation
    except (TypeError,ValueError,OverflowError):
        raise ValueError('Observation date is invalid; use YYYY-MM-DD, a FITS DD/MM/YY date, or a Julian year') from None


def reference_sky(table,header,params):
    """Share epoch handling for calibration and catalogue overlays. / 共用历元处理。"""
    import numpy as np
    import astropy.units as u
    from astropy.coordinates import SkyCoord
    from astropy.time import Time
    sky=SkyCoord(np.asarray(table['ra'],dtype=float)*u.deg,np.asarray(table['dec'],dtype=float)*u.deg,frame='icrs')
    note='Catalogue reference epoch; observation epoch unavailable'
    epoch=params.get('observation_epoch',header.get('DATE-OBS'))
    if epoch is not None and str(epoch).strip() and {'pmra','pmdec','ref_epoch'}<=set(table.colnames):
        observation=observation_time(epoch)
        pmra=np.ma.asarray(table['pmra'],dtype=float).filled(np.nan);pmdec=np.ma.asarray(table['pmdec'],dtype=float).filled(np.nan)
        epochs=np.ma.asarray(table['ref_epoch'],dtype=float).filled(np.nan);valid=np.isfinite(pmra)&np.isfinite(pmdec)&np.isfinite(epochs)
        if valid.any():
            moved=SkyCoord(sky[valid],pm_ra_cosdec=pmra[valid]*u.mas/u.yr,pm_dec=pmdec[valid]*u.mas/u.yr,
                           obstime=Time(epochs[valid],format='jyear')).apply_space_motion(new_obstime=observation)
            ra,dec=sky.ra.deg.copy(),sky.dec.deg.copy();ra[valid],dec[valid]=moved.ra.deg,moved.dec.deg
            sky=SkyCoord(ra*u.deg,dec*u.deg,frame='icrs')
            note=f'Proper motion propagated to {observation.isot}; {int((~valid).sum())} rows lack a usable motion'
        else:note='No usable proper motions; catalogue positions retained at their reference epochs'
    return sky,note


def detected_points(worker, image, params):
    import numpy as np
    from fitsviewer_detection import detect_stars
    path, hdu, header, data = image

    class CachedImage:
        def image(self, _params, data=False):
            return path, hdu, header, image[3] if data else None

    sources = []
    # Bounded overlapping tiles avoid resampling narrow stellar PSFs.
    # 有限大小的重叠分块保留原像素，避免下采样窄星点。
    for y in range(0, data.shape[0], 1952):
        for x in range(0, data.shape[1], 1952):
            checkpoint(params)
            roi = [max(0, x - 16), max(0, y - 16), min(data.shape[1], x + 1968), min(data.shape[0], y + 1968)]
            result = detect_stars(CachedImage(), dict(params, roi=roi, limit=100,
                                                       threshold_sigma=params.get('threshold_sigma', 6.),
                                                       fwhm=params.get('fwhm', 3.)))
            sources.extend(result['sources'])
    sources.sort(key=lambda source: source.get('peak', source.get('flux', 0)), reverse=True)
    selected = []
    for source in sources:
        point = np.array([source['x'], source['y']], dtype=float)
        if not any(np.linalg.norm(point - previous) < 3 for previous in selected):
            selected.append(point)
        if len(selected) >= 160:
            break
    if len(selected) < 8:
        raise ValueError('At least eight finite stellar detections are needed for Gaia calibration')
    return np.asarray(selected)


def solve(worker, params):
    maximum = float(params.get('max_residual_arcsec', 3.))
    if not math.isfinite(maximum) or not .02 <= maximum <= 10:
        raise ValueError('Maximum residual must be between 0.02 and 10 arcsec')
    checkpoint(params)
    require('numpy', 'astropy', 'scipy')
    import numpy as np
    import astropy.units as u
    from astropy.coordinates import SkyCoord, FK5
    from astropy.io import fits
    from astropy.time import Time
    from astropy.wcs.utils import fit_wcs_from_points, pixel_to_skycoord, skycoord_to_pixel
    from scipy.spatial import cKDTree

    image = worker.image(params, data=True)
    path, hdu, header, data = image
    height, width = data.shape
    prior, center, radius = prior_wcs(header, width, height, params)
    if prior is None:
        return {'accepted': False, 'requires_hint': True,
                'reason': 'Provide approximate RA, Dec and pixel scale, or an image with usable celestial WCS. No blind all-sky index is bundled.'}
    checkpoint(params)
    table, catalogue_source, catalogue_artifact = reference_table(worker, center, radius, params)
    if not {'ra', 'dec'} <= set(table.colnames):
        raise ValueError('Gaia reference table needs ra and dec columns in degrees')
    valid = np.isfinite(np.ma.asarray(table['ra'], dtype=float).filled(np.nan)) & np.isfinite(np.ma.asarray(table['dec'], dtype=float).filled(np.nan))
    table = table[valid][:5000]
    if len(table) < 8:
        raise ValueError('Too few valid Gaia reference stars in this field')
    sky,epoch_note=reference_sky(table,header,params)
    x, y = skycoord_to_pixel(sky, prior, origin=0, mode='all')
    finite = np.isfinite(x) & np.isfinite(y) & (x > -width * .4) & (x < width * 1.4) & (y > -height * .4) & (y < height * 1.4)
    table, sky = table[finite], sky[finite]
    targets = np.column_stack((x[finite], y[finite]))
    if len(targets) < 8:
        raise ValueError('Gaia references do not overlap the supplied pointing/scale')
    sources = detected_points(worker, image, params)
    tree = cKDTree(targets)
    distance, index = tree.query(sources)
    tolerance = float(params.get('match_radius_pixels', 8.))
    if not .5 <= tolerance <= 64:
        raise ValueError('Initial match radius must be between 0.5 and 64 pixels')
    pairs = []
    used = set()
    for i in np.argsort(distance):
        if distance[i] <= tolerance and int(index[i]) not in used:
            pairs.append((int(i), int(index[i])))
            used.add(int(index[i]))
    method = 'unique nearest matches from the initial WCS'
    if len(pairs) < 10:
        require('astroalign')
        import astroalign
        checkpoint(params)
        try:
            _, (control_source, control_target) = astroalign.find_transform(sources[:80], targets[:160], max_control_points=60)
        except Exception as error:
            raise ValueError('No reliable Gaia star pattern match; check pointing, scale and stellar detections') from error
        _, source_index = cKDTree(sources).query(control_source)
        _, reference_index = tree.query(control_target)
        pairs = list(dict.fromkeys(zip(source_index.tolist(), reference_index.tolist())))
        method = 'external astroalign star-pattern matching within the supplied Gaia field'
    if len(pairs) < 10:
        raise ValueError('At least ten unique Gaia matches are required, including independent validation stars')
    pixel = sources[[a for a, _ in pairs]]
    reference = sky[[b for _, b in pairs]]
    if np.linalg.matrix_rank(pixel - pixel.mean(axis=0)) < 2 or np.prod(np.ptp(pixel, axis=0)) < width * height * .015:
        raise ValueError('Matched stars lack sufficient two-dimensional field coverage')
    rng = np.random.default_rng(20261017)
    order = rng.permutation(len(pairs))
    holdout = np.zeros(len(pairs), dtype=bool)
    holdout[order[:max(3, len(pairs) // 4)]] = True
    keep = ~holdout
    fitted = None
    for _ in range(4):
        checkpoint(params)
        if keep.sum() < 6:
            raise ValueError('Too few unambiguous stars remain after residual rejection')
        # Astropy 7/8 implementation is verified with zero-based detector samples.
        # 用独立解析真值核对过像素原点，不能在此再加 1。
        fitted = fit_wcs_from_points((pixel[keep, 0], pixel[keep, 1]), reference[keep], projection='TAN')
        checkpoint(params)
        residual = pixel_to_skycoord(pixel[:, 0], pixel[:, 1], fitted, origin=0).separation(reference).arcsec
        median = np.median(residual[keep])
        mad = np.median(np.abs(residual[keep] - median)) * 1.4826
        new = (~holdout) & (residual <= max(maximum, median + 4 * mad))
        if np.array_equal(new, keep):
            break
        keep = new
    rms = float(np.sqrt(np.mean(residual[keep] ** 2)))
    validation_rms = float(np.sqrt(np.mean(residual[holdout] ** 2)))
    if rms > maximum or validation_rms > maximum or np.max(residual[holdout]) > maximum * 2:
        raise ValueError(f'Gaia calibration rejected: fit RMS {rms:.3f}, holdout RMS {validation_rms:.3f} arcsec')
    solved_center = pixel_to_skycoord((width - 1) / 2, (height - 1) / 2, fitted, origin=0)
    if solved_center.separation(center).deg > radius:
        raise ValueError('Gaia solution is inconsistent with the supplied search field')
    solution = fitted.to_header(relax=True)
    checkpoint(params)
    j2000 = solved_center.transform_to(FK5(equinox=Time('J2000')))
    matches = [{'x': float(point[0]), 'y': float(point[1]), 'source_id': str(table['source_id'][pairs[i][1]]) if 'source_id' in table.colnames else str(pairs[i][1]),
                'ra_deg': float(reference[i].icrs.ra.deg), 'dec_deg': float(reference[i].icrs.dec.deg),
                'residual_arcsec': float(residual[i]), 'validation': bool(holdout[i]), 'used_in_fit': bool(keep[i])}
               for i, point in enumerate(pixel)]
    result = {'accepted': True, 'requires_hint': False, 'catalogue': catalogue_source, 'matched': len(pairs),
              'document_key': params.get('_coordinate_key'), 'document_generation': params.get('_document_generation', 0), 'hdu': int(hdu),
              'fit_stars': int(keep.sum()), 'validation_stars': int(holdout.sum()), 'rms_arcsec': rms,
              'validation_rms_arcsec': validation_rms, 'solution_header': solution.tostring(sep='\n'),
              'center_j2000': {'ra_deg': float(j2000.ra.deg), 'dec_deg': float(j2000.dec.deg), 'sexagesimal': j2000.to_string('hmsdms', precision=3)},
              'matches': matches, 'method': method, 'coordinate_origin': 0, 'epoch_note': epoch_note,
              'original_file_modified': False, 'distortion_model': 'TAN; validation covers the matched field, no arbitrary distortion is inferred'}
    if catalogue_artifact:
        result['reference_artifact'] = catalogue_artifact
    output = worker.cache / ('gaia-solution-' + uuid.uuid4().hex + '.json')
    import json
    try:
        checkpoint(params)
        output.write_text(json.dumps(clean(result), indent=2), encoding='utf-8')
        checkpoint(params)
        result['solution_artifact'] = worker.artifact(output, 'json')
    except BaseException:
        output.unlink(missing_ok=True)
        raise
    return clean(result)
