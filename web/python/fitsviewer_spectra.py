"""Opt-in spectral feature finding in the selected Python environment.

仅按请求导入 Specutils/Astropy；返回候选极值，不作化学鉴定或盲红移求解。
"""
import math
from fitsviewer_science import require, clean


def unit_factor(unit):
    name=str(unit).strip().lower()
    if name in ('angstrom','angstroms','aa','a','å'):return 1.
    if name in ('nm','nanometer','nanometers'):return 10.
    if name in ('um','µm','μm','micron','microns'):return 10000.
    raise ValueError('Choose Angstrom, nm or um explicitly; sample indices cannot be matched')


def detect(worker,params):
    """Read the same core packet and selected columns used by the visible plot."""
    factor=unit_factor(params.get('unit',''))
    sigma=float(params.get('sigma',5.));window=int(params.get('continuum_window',101))
    method=params.get('method','derivative')
    if not math.isfinite(sigma) or not 1<=sigma<=30:raise ValueError('Noise factor must be between 1 and 30')
    if not 5<=window<=1001:raise ValueError('Continuum window must be 5 to 1001 samples')
    if method not in ('derivative','threshold'):raise ValueError('Choose derivative or threshold finding')
    require('numpy','astropy','scipy','specutils')
    import numpy as np
    import astropy.units as u
    import astropy
    import specutils
    from astropy.nddata import StdDevUncertainty
    from astropy.stats import mad_std
    from scipy.ndimage import median_filter
    try:
        from specutils import Spectrum
        from specutils.fitting import find_lines_derivative, find_lines_threshold
    except ImportError as error:
        raise ModuleNotFoundError("Selected environment needs Specutils >=2,<3 with Spectrum and line finders. Install in that environment: python -m pip install 'specutils>=2,<3'") from error
    worker.check_cancelled()
    dataset=worker.dataset(params);meta=dataset['meta'].get('spectrum')
    if meta is None or 'x' not in dataset:raise ValueError('Open a one-dimensional spectrum first')
    label=meta.get('x_label','').strip().lower();source_unit=meta.get('x_unit','').strip().lower()
    if any(name in label for name in ('sample','pixel','index','frequency','velocity','energy')) or source_unit.endswith('hz') or source_unit in ('pixel','pix','index','s','sec','km/s','m/s','ev','kev'):
        raise ValueError('Sample indices or non-wavelength axes cannot be matched')
    x=np.asarray(dataset['x'],dtype=float)*factor;y=np.asarray(dataset['values'],dtype=float)
    if x.ndim!=1 or y.ndim!=1 or len(x)!=len(y) or not 16<=len(x)<=1000000:raise ValueError('Expected 16 to 1000000 paired spectral samples')
    error=dataset.get('error')
    if error is not None:error=np.asarray(error,dtype=float)
    if error is not None and error.shape!=y.shape:raise ValueError('Uncertainty shape differs from spectrum')
    if method=='threshold' and error is None:raise ValueError('Threshold finding requires the selected standard-deviation error column; use derivative otherwise')
    valid=np.isfinite(x)&np.isfinite(y)&(x>0)
    if method=='threshold':valid&=np.isfinite(error)&(error>0)
    indices=np.flatnonzero(valid)
    if len(indices)<16:raise ValueError('Too few finite positive wavelength samples')
    # Preserve monotonic ordering and gaps; never join masked pieces into a fake feature.
    # 保持原采样次序和掩膜间隙，不将不相邻的有效点拼成伪谱线。
    differences=np.diff(x[indices]);descending=False
    if np.all(differences<0):
        x=x[::-1];y=y[::-1];valid=valid[::-1];descending=True
        if error is not None:error=error[::-1]
        indices=np.flatnonzero(valid);differences=np.diff(x[indices])
    if not np.all(differences>0):raise ValueError('Wavelength coordinates must be strictly monotonic without duplicates')
    step=float(np.median(differences))
    breaks=np.flatnonzero((np.diff(indices)>1)|(differences>5*step))+1
    segments=np.split(indices,breaks)
    found=[];used=0;noise_methods=set();windows=set()
    for segment in segments:
        worker.check_cancelled()
        if len(segment)<16:continue
        sx=x[segment];sy=y[segment]
        size=min(window if window%2 else window+1,len(segment) if len(segment)%2 else len(segment)-1)
        continuum=median_filter(sy,size=size,mode='nearest');residual=sy-continuum
        uncertainty=None
        if error is not None:
            se=error[segment];positive=se[np.isfinite(se)&(se>0)]
        else:positive=[]
        if len(positive)>=max(8,len(segment)//2):
            noise=float(np.median(positive));noise_methods.add('median source standard deviation')
        else:
            noise=float(mad_std(np.diff(residual),ignore_nan=True)/np.sqrt(2.));noise_methods.add('MAD of adjacent residual differences')
        if not math.isfinite(noise) or noise<=0:continue
        if method=='threshold':uncertainty=StdDevUncertainty(error[segment])
        spectrum=Spectrum(spectral_axis=sx*u.AA,flux=residual*u.dimensionless_unscaled,uncertainty=uncertainty)
        worker.check_cancelled()
        table=find_lines_derivative(spectrum,flux_threshold=sigma*noise) if method=='derivative' else find_lines_threshold(spectrum,noise_factor=sigma)
        worker.check_cancelled()
        for row in table:
            local=int(row['line_center_index']);center=float(row['line_center'].to_value(u.AA))
            if not math.isfinite(center) or local<1 or local>=len(segment)-1:continue
            source_index=int(segment[local])
            if descending:source_index=len(x)-1-source_index
            found.append({'center_angstrom':center,'kind':str(row['line_type']),
                          'sample_index':source_index,'residual':float(residual[local]),
                          'noise_scale':noise,'contrast_over_noise':float(abs(residual[local])/noise)})
        used+=len(segment);windows.add(size)
    if not used:raise ValueError('No usable segment with a positive noise estimate; choose a suitable error column or noisy spectrum')
    found.sort(key=lambda row:row['contrast_over_noise'],reverse=True)
    total=len(found);found=sorted(found[:256],key=lambda row:row['center_angstrom'])
    return clean({'operation':'spectral_candidates','source':params.get('source',''),'unit':params['unit'],'air':bool(params.get('air',False)),
                  'candidates':found,'total_candidates':total,'truncated':total>256,'samples_used':used,
                  'method':method,'sigma':sigma,'continuum':'running median subtracted','continuum_windows':sorted(windows),
                  'noise_methods':sorted(noise_methods),'versions':{'astropy':astropy.__version__,'specutils':specutils.__version__},
                  'interpretation':'Provisional extrema only; line proximity at user z is not chemical identification or a redshift solution.'})
