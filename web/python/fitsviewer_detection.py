"""Shared original star detection for Gaia and PRO. / Gaia 与 PRO 共用原始找星实现。"""
import math
from fitsviewer_science import require,clean
def roi(worker,params):
    import numpy as np
    _,_,header,data=worker.image(params,data=True)
    bounds=params.get('roi',[0,0,data.shape[1],data.shape[0]])
    x0,y0,x1,y1=[int(v) for v in bounds]
    if not 0<=x0<x1<=data.shape[1] or not 0<=y0<y1<=data.shape[0] or (x1-x0)*(y1-y0)>4*1024*1024:raise ValueError('Choose a nonempty ROI of at most 4 Mi pixels inside the image')
    values=data[y0:y1,x0:x1]
    if params.get('mask_path'):
        _,_,_,mask=worker.image({'path':params['mask_path'],'hdu':params.get('mask_hdu')},data=True)
        if mask.shape!=data.shape:raise ValueError('Detection mask dimensions differ from the image')
        values=np.where(np.isfinite(mask[y0:y1,x0:x1]) & (mask[y0:y1,x0:x1]==0),values,np.nan)
    if np.count_nonzero(np.isfinite(values))<16:raise ValueError('Too few finite pixels in the ROI')
    return header,values,(x0,y0)

def detect_stars(worker,params):
    require('numpy','astropy','scipy')
    import numpy as np
    from scipy.ndimage import gaussian_filter,maximum_filter
    from astropy.stats import sigma_clipped_stats
    _,values,(x0,y0)=roi(worker,params)
    mean,background,noise=sigma_clipped_stats(values,sigma=3,maxiters=5)
    if not np.isfinite(noise) or noise<=0:raise ValueError('Cannot estimate positive background noise; supply a noisy ROI')
    fwhm=float(params.get('fwhm',3.));threshold=float(params.get('threshold_sigma',5.));separation=float(params.get('min_separation',fwhm))
    if not .5<=fwhm<=50 or not 1<=threshold<=100 or not .5<=separation<=100:raise ValueError('Invalid FWHM, threshold or minimum separation')
    sigma=fwhm/2.354820045;finite=np.isfinite(values);filled=np.where(finite,values-background,0.)
    smoothed=gaussian_filter(filled,sigma,mode='constant',cval=0.)
    kernel=np.exp(-np.arange(-math.ceil(4*sigma),math.ceil(4*sigma)+1,dtype=float)**2/(2*sigma*sigma));kernel/=kernel.sum();filtered_noise=float(noise*(kernel*kernel).sum())
    size=2*math.ceil(separation)+1
    peak=(smoothed==maximum_filter(smoothed,size=size,mode='constant',cval=0))&(smoothed>threshold*filtered_noise)&finite
    if params.get('saturation') is not None:peak&=values<float(params['saturation'])
    yy,xx=np.nonzero(peak);order=np.argsort(smoothed[yy,xx])[::-1][:20000]
    radius=max(2,math.ceil(1.5*fwhm));sources=[];centers=[];limit=max(1,min(int(params.get('limit',2000)),10000))
    for index in order:
        x=int(xx[index]);y=int(yy[index])
        if any((x-a)**2+(y-b)**2<separation*separation for a,b in centers):continue
        left=max(0,x-radius);right=min(values.shape[1],x+radius+1);bottom=max(0,y-radius);top=min(values.shape[0],y+radius+1)
        block=values[bottom:top,left:right];gy,gx=np.mgrid[bottom:top,left:right]
        weight=np.maximum(np.where(np.isfinite(block),block-background,0.),0.)
        total=weight.sum()
        if total<=0:continue
        cx=float((gx*weight).sum()/total);cy=float((gy*weight).sum()/total)
        vx=float(((gx-cx)**2*weight).sum()/total);vy=float(((gy-cy)**2*weight).sum()/total)
        if not np.isfinite([cx,cy,vx,vy]).all():continue
        flags=[]
        if not np.isfinite(block).all():flags.append('masked_pixels_near_source; centroid may be biased')
        if left==0 or bottom==0 or right==values.shape[1] or top==values.shape[0]:flags.append('edge')
        if params.get('saturation') is not None and np.nanmax(block)>=float(params['saturation']):continue
        sources.append({'id':len(sources),'x':cx+x0,'y':cy+y0,'peak':float(values[y,x]),'background':float(background),'aperture_signal_estimate':float(total),
                        'fwhm_moment_estimate':2.354820045*math.sqrt(max(0.,(vx+vy)/2)),'roundness_estimate':(vx-vy)/max(vx+vy,1e-30),'flags':flags})
        centers.append((cx,cy))
        if len(sources)>=limit:break
    return {'sources':sources,'result_type':'local_intensity_candidates','catalogue_identification':False,'scientific_note':'Local intensity maxima only; use catalogue identification for catalogue identities.',
            'background':background,'background_sigma':noise,'convolved_sigma':filtered_noise,'fwhm_input':fwhm,'threshold_sigma':threshold,
            'method':'Gaussian-filtered local maxima with positive-weight centroids','width_is_psf_fit':False,'crowded_field_deblending':False,'coordinate_origin':0,'truncated':len(sources)>=limit}
