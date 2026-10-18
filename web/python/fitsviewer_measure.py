"""Optional detection/coordinate tools using packages in the selected environment.
检测与坐标适配：仅按用户请求导入已有科研包，不包含模型权重或 OpenCV。
"""
import math
from fitsviewer_science import require,clean
from fitsviewer_detection import roi,detect_stars



def detect_lines(worker,params):
    require('numpy','astropy','scipy','skimage')
    import numpy as np
    from scipy.ndimage import gaussian_filter
    from skimage.transform import probabilistic_hough_line
    from astropy.stats import sigma_clipped_stats
    _,values,(x0,y0)=roi(worker,params)
    _,background,noise=sigma_clipped_stats(values,sigma=3,maxiters=5)
    sigma=float(params.get('smooth_sigma',1.));threshold=float(params.get('threshold_sigma',4.));seed=int(params.get('seed',20261017))
    binary=gaussian_filter(np.where(np.isfinite(values),values-background,0.),sigma)>threshold*noise
    lines=probabilistic_hough_line(binary,threshold=int(params.get('votes',12)),line_length=int(params.get('min_length',20)),line_gap=int(params.get('gap',4)),rng=seed)
    lines.sort(key=lambda line:math.dist(*line),reverse=True);result=[]
    for a,b in lines[:min(int(params.get('limit',50)),200)]:
        a=np.asarray(a,dtype=float);b=np.asarray(b,dtype=float);length=float(np.linalg.norm(b-a))
        if length==0:continue
        samples=np.linspace(a,b,max(2,math.ceil(length))).round().astype(int);hit=float(binary[samples[:,1],samples[:,0]].mean())
        result.append({'start':(a+[x0,y0]).tolist(),'end':(b+[x0,y0]).tolist(),'length':length,'angle_deg':math.degrees(math.atan2(b[1]-a[1],b[0]-a[0])),'ridge_hit_fraction':hit,'width_estimate':float(params.get('width',3.))})
    return {'lines':result,'seed':seed,'method':'probabilistic Hough on background-subtracted threshold candidates','coordinate_origin':0,'confidence_is_probability':False}

def trace_curve(worker,params):
    require('numpy','astropy')
    import numpy as np
    from astropy.stats import sigma_clipped_stats
    _,values,(x0,y0)=roi(worker,params)
    vertical=params.get('axis','x')=='y'
    if vertical:values=values.T
    seed=np.asarray(params['seed'],dtype=float)-[x0,y0]
    if vertical:seed=seed[::-1]
    if seed.shape!=(2,) or not np.isfinite(seed).all() or not 0<=seed[0]<=values.shape[1]-1 or not 0<=seed[1]<=values.shape[0]-1:
        raise ValueError('Trace seed must be a finite pixel center inside the selected image/ROI')
    half=float(params.get('search_half_width',8.));width=float(params.get('centroid_half_width',3.));order=int(params.get('order',3));threshold=float(params.get('threshold_sigma',3.));step=int(params.get('step',1));direction=int(params.get('direction',1))
    if not 1<=half<=100 or not .5<=width<=half or not 1<=order<=5 or not 1<=step<=16 or direction not in [-1,1]:raise ValueError('Invalid constrained trace parameters')
    _,background,noise=sigma_clipped_stats(values,sigma=3,maxiters=5)
    current=float(seed[1]);accepted=[];rejected=[];gap=0
    for x in range(int(round(seed[0])),values.shape[1] if direction>0 else -1,direction*step):
        cancel=params.get('_cancel')
        if cancel and cancel.is_set():raise InterruptedError('Trace cancelled')
        if len(accepted)>=4:
            recent=np.asarray(accepted[-8:]);prediction=np.polyval(np.polyfit(recent[:,0],recent[:,1],min(2,len(recent)-1)),x)
            if np.isfinite(prediction) and abs(prediction-current)<half:current=float(prediction)
        lower=max(0,int(math.floor(current-half)));upper=min(values.shape[0],int(math.ceil(current+half))+1)
        column=values[lower:upper,x]-background
        if not len(column) or not np.isfinite(column).any():rejected.append([x,current,'no finite samples']);gap+=1
        else:
            peak=int(np.nanargmax(column))+lower
            lo=max(lower,int(math.floor(peak-width)));hi=min(upper,int(math.ceil(peak+width))+1);yy=np.arange(lo,hi,dtype=float);weights=np.maximum(np.nan_to_num(values[lo:hi,x]-background,nan=0.),0.)
            if weights.sum()<=0 or np.nanmax(column)<threshold*noise:rejected.append([x,current,'below threshold']);gap+=1
            else:
                center=float((yy*weights).sum()/weights.sum())
                if abs(center-current)>half:rejected.append([x,center,'continuity limit']);gap+=1
                else:accepted.append([float(x),center]);current=center;gap=0
        if gap>=int(params.get('max_gap',8)):break
    if len(accepted)<order+2:raise ValueError('Too few points for the requested polynomial order; adjust the seed/search band')
    samples=np.asarray(accepted);keep=np.ones(len(samples),dtype=bool)
    for _ in range(4):
        coefficients=np.polyfit(samples[keep,0],samples[keep,1],order);residual=samples[:,1]-np.polyval(coefficients,samples[:,0]);sigma=np.std(residual[keep]);new=np.abs(residual)<=max(3*sigma,.25)
        if new.sum()<order+2 or np.array_equal(new,keep):break
        keep=new
    for p in samples[~keep]:rejected.append([float(p[0]),float(p[1]),'fit residual'])
    trace=np.column_stack((samples[keep,0],np.polyval(coefficients,samples[keep,0])))
    if vertical:trace=trace[:,::-1]
    trace+=[x0,y0]
    def global_point(point):
        xy=point[:2][::-1] if vertical else point[:2]
        return [float(xy[0]+x0),float(xy[1]+y0)]
    accepted_global=[global_point(point) for point in samples[keep]]
    rejected_global=[global_point(point)+[point[2]] for point in rejected]
    return {'points':trace.tolist(),'accepted':accepted_global,'rejected':rejected_global,'coefficients':coefficients.tolist(),'coefficient_coordinates':'ROI local, independent axis -> dependent axis','roi_origin':[x0,y0],'axis':'y' if vertical else 'x','order':order,'rms_pixels':float(np.sqrt(np.mean(residual[keep]**2))),
            'method':'user-seeded constrained centroid tracking and sigma-rejected polynomial','automatic_arbitrary_curve_detection':False,'coordinate_origin':0}

def boundary(shape):
    import numpy as np
    kind=shape['kind'];t=np.linspace(0,2*np.pi,129)[:-1]
    if kind in ['circle','ring','ellipse']:
        center=np.asarray(shape['center']);angle=np.deg2rad(shape.get('angle_deg',0));radii=shape.get('radii',[shape.get('radius',shape.get('outer'))]*2)
        vertices=np.column_stack((radii[0]*np.cos(t),radii[1]*np.sin(t)))@np.array([[np.cos(angle),np.sin(angle)],[-np.sin(angle),np.cos(angle)]])+center
        holes=[]
        if kind=='ring':holes=[np.column_stack((shape['inner']*np.cos(t),shape['inner']*np.sin(t)))+center]
        return vertices,holes
    if kind=='rectangle':
        center=np.asarray(shape['center']);sx,sy=np.asarray(shape['size'])/2;angle=np.deg2rad(shape.get('angle_deg',0));vertices=np.array([[-sx,-sy],[sx,-sy],[sx,sy],[-sx,sy]])@np.array([[np.cos(angle),np.sin(angle)],[-np.sin(angle),np.cos(angle)]])+center
        return vertices,[]
    if kind=='polygon':return np.asarray(shape['points']),[np.asarray(h) for h in shape.get('holes',[])]
    raise ValueError('Sky paste of a finite-width path needs its polygon boundary; convert it to a polygon first')

def region_transform(worker,params):
    require('numpy','astropy')
    import numpy as np
    from astropy.io import fits
    from astropy.wcs import WCS
    from astropy.wcs.utils import pixel_to_skycoord,skycoord_to_pixel
    template=params['template']
    if not template.get('source_wcs_complete',True):raise ValueError('This template requires external distortion lookup arrays; use pixel paste or retain the source WCS dataset')
    header=fits.Header.fromstring(template['source_header'],sep='\n');source=WCS(header,naxis=2).celestial
    if not source.has_celestial:raise ValueError('Source template has no celestial WCS')
    target_header,target=worker.celestial(params)
    def transform(shape):
        if shape['kind']=='union':return {'kind':'union','shapes':[transform(s) for s in shape['shapes']]}
        if shape['kind']=='path':
            require('shapely')
            from shapely.geometry import LineString
            points=np.asarray(shape['points'],dtype=float)
            if shape.get('smooth'):
                vertices=[]
                for i in range(len(points)-1):
                    p0=points[max(i-1,0)];p1=points[i];p2=points[i+1];p3=points[min(i+2,len(points)-1)]
                    for t in np.linspace(0,1,max(8,min(64,math.ceil(np.linalg.norm(p2-p1)))) ,endpoint=False):vertices.append(.5*(2*p1+(-p0+p2)*t+(2*p0-5*p1+4*p2-p3)*t*t+(-p0+3*p1-3*p2+p3)*t*t*t))
                vertices.append(points[-1]);points=np.asarray(vertices)
            line=LineString(points);polygon=line.buffer(shape['width']/2,cap_style=2,join_style=1)
            if shape.get('inner_width',0)>0:polygon=polygon.difference(line.buffer(shape['inner_width']/2,cap_style=2,join_style=1))
            polygons=list(polygon.geoms) if polygon.geom_type=='MultiPolygon' else [polygon]
            result=[]
            for polygon in polygons:
                result.append(transform({'kind':'polygon','points':list(polygon.exterior.coords),'holes':[list(r.coords) for r in polygon.interiors]}))
            return result[0] if len(result)==1 else {'kind':'union','shapes':result}
        outer,holes=boundary(shape)
        def convert(points):
            dense=[]
            lengths=np.linalg.norm(np.roll(points,-1,axis=0)-points,axis=1);budget=max(1.,float(lengths.sum())/1500)
            for a,b,length in zip(points,np.roll(points,-1,axis=0),lengths):
                steps=max(1,math.ceil(length/max(4.,budget)))
                for t in np.linspace(0,1,steps,endpoint=False):dense.append(a+t*(b-a))
            points=np.asarray(dense)
            if len(points)>2048:raise ValueError('Region boundary exceeds 2048 vertices')
            sky=pixel_to_skycoord(points[:,0],points[:,1],source,origin=0,mode='all');x,y=skycoord_to_pixel(sky,target,origin=0,mode='all');out=np.column_stack((x,y))
            if not np.isfinite(out).all():raise ValueError('Region crosses an invalid projection boundary; paste cancelled')
            return out.tolist()
        return {'kind':'polygon','points':convert(outer),'holes':[convert(h) for h in holes]}
    apertures=[]
    for aperture in template['apertures']:
        apertures.append({'name':aperture['name'],'shape':transform(aperture['shape']),'background':transform(aperture['background']) if aperture.get('background') else None,'exclude':[transform(s) for s in aperture.get('exclude',[])]})
    return {'schema':1,'source_dimensions':[target_header['NAXIS1'],target_header['NAXIS2']],'source_hdu':params.get('hdu',0),'source_header':target.to_header(relax=True).tostring(sep='\n'),'apertures':apertures,'options':template.get('options'),'source_units':target_header.get('BUNIT',''),'source_wcs_complete':not any(key.startswith(('CPDIS','D2IM','DET2IM')) for key in target_header),'method':'source WCS -> sky -> target WCS, dense boundary projection','remeasure_required':True}

def line_profile(worker,params):
    require('numpy','astropy','scipy')
    import numpy as np
    from scipy.ndimage import map_coordinates
    _,_,_,data=worker.image(params,data=True);points=np.asarray(params['points'],dtype=float);width=float(params.get('width',3.));spacing=float(params.get('spacing',1.))
    if points.ndim!=2 or points.shape[1]!=2 or not 2<=len(points)<=512 or not np.isfinite(points).all() or not .1<=width<=256 or not .25<=spacing<=16:raise ValueError('Invalid path/profile width or spacing')
    distance=np.r_[0.,np.cumsum(np.linalg.norm(np.diff(points,axis=0),axis=1))]
    if distance[-1]<=0 or distance[-1]/spacing>8192:raise ValueError('Profile needs a finite path of at most 8192 samples')
    position=np.linspace(0,distance[-1],max(2,math.ceil(distance[-1]/spacing)+1));x=np.interp(position,distance,points[:,0]);y=np.interp(position,distance,points[:,1]);dx=np.gradient(x);dy=np.gradient(y);norm=np.hypot(dx,dy);nx=-dy/np.maximum(norm,1e-15);ny=dx/np.maximum(norm,1e-15)
    def strip(offsets):
        xx=x[:,None]+nx[:,None]*offsets;yy=y[:,None]+ny[:,None]*offsets
        return map_coordinates(data,[yy,xx],order=1,mode='constant',cval=np.nan,prefilter=False)
    offsets=np.linspace(-width/2,width/2,max(2,math.ceil(width*8)),endpoint=False)+width/(2*max(2,math.ceil(width*8)))
    values=strip(offsets);mean=np.nanmean(values,axis=1);coverage=np.isfinite(values).mean(axis=1)
    background=np.zeros(len(mean));gap=float(params.get('background_gap',2.));bgwidth=float(params.get('background_width',0.))
    if not math.isfinite(gap) or not 0<=gap<=512 or not math.isfinite(bgwidth) or not 0<=bgwidth<=256:raise ValueError('Profile background gap/width must be finite and non-negative within the supported bounds')
    if bgwidth>0:
        offsets=np.linspace(width/2+gap,width/2+gap+bgwidth,max(2,math.ceil(bgwidth*8)));background=np.nanmean(strip(np.r_[-offsets,offsets]),axis=1)
    net=(mean-background)*width*coverage;fwhm=None
    if np.isfinite(net).any():
        peak=int(np.nanargmax(net));half=net[peak]/2
        left=next((i for i in range(peak-1,-1,-1) if np.isfinite(net[i:i+2]).all() and net[i]<=half<net[i+1]),None)
        right=next((i for i in range(peak,len(net)-1) if np.isfinite(net[i:i+2]).all() and net[i]>=half>net[i+1]),None)
        if half>0 and left is not None and right is not None:
            a=position[left]+(half-net[left])/(net[left+1]-net[left])*(position[left+1]-position[left]);b=position[right]+(net[right]-half)/(net[right]-net[right+1])*(position[right+1]-position[right]);fwhm=float(b-a)
    return clean({'distance_pixels':position,'x':x,'y':y,'sampled_mean':mean,'background_mean':background,'boxcar_net':net,'valid_fraction':coverage,'width_pixels':width,'fwhm_estimate_pixels':fwhm,'fwhm_method':'nearest half-height crossings of the strongest positive peak; not a PSF/seeing fit',
                  'profile_method':'bilinear transverse sampling; independent sample windows may overlap','band_aperture_total_is_separate':True,'wavelength_calibrated':False})

def request(worker,operation,params):
    return globals()[operation](worker,params)
