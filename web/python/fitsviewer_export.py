"""Optional Agg/MathText export through the selected environment, never bundled TeX.
通过用户环境中的 Matplotlib/Astropy 导出；明确关闭 TeX，不捆绑排版引擎。
"""
from pathlib import Path
import json
import math
import uuid
from fitsviewer_science import require,clean
import os

def display_limits(data,view):
    import numpy as np
    if not np.isfinite(data).any():low,high=0.,1.
    else:
        low=float(view['low']) if 'low' in view else float(np.nanpercentile(data,.25))
        high=float(view['high']) if 'high' in view else float(np.nanpercentile(data,99.75))
    if not math.isfinite(low) or not math.isfinite(high) or high<=low:raise ValueError('Export requires finite Low < High')
    return low,high

def histogram_cdf(data,low,high):
    """Fixed-size image CDF shared with the colorbar. / 图像与色条共享固定大小CDF。"""
    import numpy as np
    counts=np.zeros(4096,dtype=np.int64)
    # Bound temporary arrays even for non-contiguous image/ROI views.
    # 分块统计，避免为整幅图像创建额外浮点/整数数组。
    for start in range(0,data.size,65536):
        block=data.flat[start:start+65536]
        selected=block[np.isfinite(block)&(block>=low)&(block<=high)]
        bins=np.floor(np.clip((selected-low)/(high-low),0,1)*4095).astype(np.intp)
        counts+=np.bincount(bins,minlength=4096)
    occupied=np.flatnonzero(counts)
    if len(occupied)<2:return np.linspace(0.,1.,4096)
    first=counts[occupied[0]]
    return np.clip((np.cumsum(counts,dtype=float)-first)/(int(counts.sum())-first),0,1)

def stretch_values(normalized,view,cdf=None):
    import numpy as np
    x=np.clip(normalized,0,1);stretch=view.get('stretch','Linear').lower()
    exponent=float(view.get('render',{}).get('stretch_exponent',1000.))
    if not math.isfinite(exponent) or not 1<exponent<=1e6:raise ValueError('Stretch exponent must be finite and 1 < exponent <= 1000000')
    if stretch=='log':return np.log1p(exponent*x)/math.log1p(exponent)
    if stretch in ('power','pow'):return np.expm1(math.log(exponent)*x)/(exponent-1)
    if stretch=='sqrt':return np.sqrt(x)
    if stretch=='squared':return x*x
    if stretch=='asinh':return np.arcsinh(10*x)/math.asinh(10)
    if stretch=='sinh':return np.sinh(3*x)/math.sinh(3)
    if stretch in ('histogram equalization','histequ'):
        if cdf is None:raise ValueError('Histogram stretch requires image CDF')
        position=np.where(np.isfinite(x),x,0.)*4095
        lower=np.floor(position).astype(np.intp);upper=np.minimum(lower+1,4095)
        result=cdf[lower]+(cdf[upper]-cdf[lower])*(position-lower)
        result=np.where(np.isfinite(x),result,np.nan)
        return np.where(x<=0,0.,np.where(x>=1,1.,result))
    if stretch!='linear':raise ValueError('Unknown display stretch')
    return x

def rgba(data,view,histogram_cdf=None):
    import numpy as np
    finite=np.isfinite(data);low,high=display_limits(data,view)
    normalized=np.clip((data-low)/(high-low),0,1)
    if view.get('stretch','Linear').lower() in ('histogram equalization','histequ') and histogram_cdf is None:
        histogram_cdf=globals()['histogram_cdf'](data,low,high)
    normalized=stretch_values(normalized,view,histogram_cdf)
    normalized=np.clip((normalized-float(view.get('bias',.5)))*float(view.get('contrast',1.))+.5,0,1)
    if view.get('invert'):normalized=1-normalized
    palettes={'Gray':[[0,0,0,255],[255,255,255,255]],'Heat':[[0,0,0,255],[150,0,0,255],[255,140,0,255],[255,255,220,255]],
              'Cool':[[0,15,35,255],[0,120,180,255],[80,220,220,255],[240,255,255,255]],'Blue-orange':[[20,45,130,255],[90,160,220,255],[235,235,235,255],[240,150,45,255],[140,45,10,255]],'Red':[[0,0,0,255],[255,70,50,255]]}
    options=view.get('render',{});palette=options.get('palette',{});colors=np.asarray(palette.get('colors',palettes.get(palette.get('name','Gray'),palettes['Gray'])),dtype=float)
    if colors.ndim!=2 or colors.shape[1]!=4 or not 2<=len(colors)<=256 or not np.isfinite(colors).all() or np.any((colors<0)|(colors>255)):raise ValueError('Invalid export palette')
    positions=np.linspace(0,1,len(colors));safe=np.where(finite,normalized,0.)
    output=np.stack([np.floor(np.interp(safe,positions,colors[:,i])+.5) for i in range(4)],axis=-1).astype('uint8');output[~finite]=[95,45,110,255]
    mask=options.get('mask')
    if mask:
        active=finite&(data>=float(mask['threshold']));opacity=float(mask.get('opacity',.5))
        if not 0<=opacity<=1:raise ValueError('Invalid mask opacity')
        if mask.get('style')=='Outline':
            interior=active.copy();interior[1:,:]&=active[:-1,:];interior[:-1,:]&=active[1:,:];interior[:,1:]&=active[:,:-1];interior[:,:-1]&=active[:,1:];interior[[0,-1],:]=False;interior[:,[0,-1]]=False;active&=~interior
        output[active,:3]=np.floor(output[active,:3]*(1-opacity)+np.asarray(mask.get('color',[255,70,30]))*opacity+.5).astype('uint8')
    return output,low,high

def annotation(ax,item,font,wcs=None,pixel_transform=None):
    from matplotlib.patches import Circle,Ellipse,Rectangle,Polygon
    kind=item.get('kind','text');color=item.get('color','yellow');width=float(item.get('line_width',1.));pixels=pixel_transform if pixel_transform is not None else ax.transData
    if kind=='text':
        text=str(item.get('text',''))
        if len(text)>4096:raise ValueError('Annotation exceeds 4096 characters')
        anchor=item.get('anchor','pixel');transform=pixels
        if anchor=='sky':
            if wcs is None or not wcs.has_celestial:raise ValueError('Sky text anchor requires celestial WCS')
            from astropy.coordinates import SkyCoord,FK5
            from astropy.time import Time
            from astropy.wcs.utils import skycoord_to_pixel
            x,y=skycoord_to_pixel(SkyCoord(float(item['ra_deg']),float(item['dec_deg']),unit='deg',frame=FK5(equinox=Time('J2000'))),wcs,origin=0)
        elif anchor=='canvas':x,y=float(item['x']),float(item['y']);transform=ax.transAxes
        elif anchor=='pixel':x,y=float(item['x']),float(item['y'])
        else:raise ValueError('Text anchor must be pixel, sky or canvas')
        ax.text(x,y,text,color=color,fontsize=float(item.get('font_size',11.)),fontproperties=font,usetex=False,transform=transform)
    elif kind=='circle':ax.add_patch(Circle(item['center'],float(item['radius']),fill=False,edgecolor=color,linewidth=width,transform=pixels))
    elif kind=='ellipse':ax.add_patch(Ellipse(item['center'],2*item['radii'][0],2*item['radii'][1],angle=item.get('angle_deg',0),fill=False,edgecolor=color,linewidth=width,transform=pixels))
    elif kind=='rectangle':
        center=item['center'];size=item['size'];ax.add_patch(Rectangle((center[0]-size[0]/2,center[1]-size[1]/2),*size,angle=item.get('angle_deg',0),rotation_point='center',fill=False,edgecolor=color,linewidth=width,transform=pixels))
    elif kind=='polygon':
        ax.add_patch(Polygon(item['points'],fill=False,edgecolor=color,linewidth=width,transform=pixels))
        for hole in item.get('holes',[]):ax.add_patch(Polygon(hole,fill=False,edgecolor=color,linewidth=width,transform=pixels))
    elif kind=='ring':
        for radius in [item['inner'],item['outer']]:ax.add_patch(Circle(item['center'],radius,fill=False,edgecolor=color,linewidth=width,transform=pixels))
    elif kind=='union':
        for shape in item['shapes']:annotation(ax,dict(shape,color=color,line_width=width),font,wcs,pixels)
    elif kind in ['path','line']:
        import numpy as np
        points=np.asarray(item.get('points',[item.get('start'),item.get('end')]),dtype=float);ax.plot(points[:,0],points[:,1],color=color,linewidth=width,transform=pixels)
    else:raise ValueError('Unsupported annotation shape')


def rotated_view(ax,data,view):
    """Use one artist transform for image and raw-pixel overlays, with no new image.

    屏幕顺时针角对应 y 向上原像素的负角；裁剪四角先映射到同一显示平面。
    """
    import numpy as np
    from matplotlib.transforms import Affine2D
    angle=float(view.get('rotation_deg',0.))
    if not math.isfinite(angle):raise ValueError('Display rotation must be finite')
    height,width=data.shape;affine=Affine2D().rotate_deg_around((width-1)/2,(height-1)/2,-angle)
    polygon=view.get('viewport_polygon');crop=view.get('crop')
    if polygon is not None:
        points=np.asarray(polygon,dtype=float)
        if points.shape!=(4,2) or not np.isfinite(points).all():raise ValueError('Viewport polygon must contain four finite original-pixel corner pairs')
    elif crop is not None:
        if len(crop)!=4 or not np.isfinite(crop).all() or crop[2]<=crop[0] or crop[3]<=crop[1]:raise ValueError('Crop must contain increasing finite pixel bounds')
        points=np.array([[crop[0],crop[1]],[crop[2],crop[1]],[crop[2],crop[3]],[crop[0],crop[3]]])
    else:points=np.array([[-.5,-.5],[width-.5,-.5],[width-.5,height-.5],[-.5,height-.5]])
    bounds=affine.transform(points)
    ax.set_xlim(float(bounds[:,0].min()),float(bounds[:,0].max()));ax.set_ylim(float(bounds[:,1].min()),float(bounds[:,1].max()))
    return affine


def rotated_wcs(wcs,affine):
    """Adapt display pixels to the exact original WCS, including distortion terms.

    WCS 轴保持矩形显示框；不旋转边框，也不丢弃 SIP/查找表畸变模型。
    """
    import numpy as np
    from astropy.wcs.wcsapi.wrappers import BaseWCSWrapper
    class DisplayWCS(BaseWCSWrapper):
        @property
        def pixel_shape(self):return None
        @property
        def pixel_bounds(self):return None
        def pixel_to_world_values(self,x,y):
            x,y=np.broadcast_arrays(x,y);raw=affine.inverted().transform(np.column_stack((x.ravel(),y.ravel())))
            return self._wcs.pixel_to_world_values(raw[:,0].reshape(x.shape),raw[:,1].reshape(y.shape))
        def world_to_pixel_values(self,*world):
            x,y=self._wcs.world_to_pixel_values(*world);x,y=np.broadcast_arrays(x,y)
            displayed=affine.transform(np.column_stack((x.ravel(),y.ravel())))
            return displayed[:,0].reshape(x.shape),displayed[:,1].reshape(y.shape)
    return DisplayWCS(wcs.low_level_wcs)

def export(worker,params):
    require('numpy','astropy','matplotlib')
    import numpy as np
    import matplotlib
    matplotlib.use('Agg',force=True)
    from matplotlib.figure import Figure
    from matplotlib.backends.backend_agg import FigureCanvasAgg
    from matplotlib.font_manager import FontProperties
    from astropy.wcs import WCS
    width=int(params.get('width',1600));height=int(params.get('height',1200));dpi=float(params.get('dpi',150));format=params.get('format','png').lower()
    if not 200<=width<=8192 or not 200<=height<=8192 or width*height>16*1024*1024 or not 36<=dpi<=1200:raise ValueError('Invalid export pixel size/DPI; maximum is 16 Mi pixels')
    if format not in ['png','jpg','jpeg','pdf','svg']:raise ValueError('Export format must be PNG, JPG, PDF or SVG')
    panels=params.get('panels',[params])
    if not 1<=len(panels)<=9:raise ValueError('Export supports one to nine panels')
    columns=max(1,min(int(params.get('columns',math.ceil(math.sqrt(len(panels))))),len(panels)));rows=math.ceil(len(panels)/columns)
    fontfile=Path(__file__).resolve().parents[1]/'fonts/cjk.ttf';font=FontProperties(fname=str(fontfile)) if fontfile.is_file() else FontProperties()
    path=worker.cache/('figure-'+uuid.uuid4().hex+'.'+format)
    provenance=[];cancel=params.get('_cancel');figure=None
    try:
        with matplotlib.rc_context({'text.usetex':False,'pdf.fonttype':42,'svg.fonttype':'path','font.size':float(params.get('font_size',10.)),'axes.unicode_minus':False}):
            figure=Figure(figsize=(width/dpi,height/dpi),dpi=dpi,layout='constrained');FigureCanvasAgg(figure)
            for index,panel in enumerate(panels):
                if cancel and cancel.is_set():raise InterruptedError('Figure export cancelled')
                dataset=worker.dataset(panel) if os.environ.get('FITSVIEWER_CORE_BINARY') else None
                if dataset is not None and dataset['meta'].get('spectrum'):
                    meta=dataset['meta']['spectrum'];view=panel.get('view',params.get('view',{}));ax=figure.add_subplot(rows,columns,index+1)
                    values=dataset['values'];coordinates=dataset['x'];valid=np.isfinite(coordinates)&np.isfinite(values)
                    ax.plot(np.where(valid,coordinates,np.nan),np.where(valid,values,np.nan),color='black',linewidth=.8)
                    if dataset.get('error') is not None and panel.get('show_errors',True):
                        uncertainty=dataset['error'];valid_error=valid&np.isfinite(uncertainty)&(uncertainty>=0)
                        ax.fill_between(coordinates,np.where(valid_error,values-uncertainty,np.nan),np.where(valid_error,values+uncertainty,np.nan),color='gray',alpha=.3,linewidth=0)
                    bounds=view.get('plot_bounds')
                    if bounds is not None:
                        if len(bounds)!=4 or not np.isfinite(bounds).all() or bounds[0]>=bounds[1] or bounds[2]>=bounds[3]:raise ValueError('Invalid spectrum plot bounds')
                        ax.set_xlim(bounds[0:2]);ax.set_ylim(bounds[2:4])
                    ax.set_xlabel(meta['x_label']+' ['+(meta['x_unit'] or 'unit unknown')+']',fontproperties=font);ax.set_ylabel(meta['y_label']+' ['+(meta['y_unit'] or 'unit unknown')+']',fontproperties=font)
                    ax.set_title(str(panel.get('title','')),fontproperties=font)
                    for item in panel.get('annotations',[]):annotation(ax,item,font)
                    provenance.append({'path':str(panel['path']),'hdu':dataset['meta']['hdu'],'view':view,'spectrum':meta,'samples':len(values),'annotations':panel.get('annotations',[])})
                    continue
                _,hdu,header,data=worker.image(panel,data=dataset is None)
                if dataset is not None:data=dataset['values']
                view=panel.get('view',params.get('view',{}));cdf=None
                if view.get('stretch','Linear').lower() in ('histogram equalization','histequ'):cdf=histogram_cdf(data,*display_limits(data,view))
                pixels,low,high=rgba(data,view,histogram_cdf=cdf)
                wcs=WCS(header,naxis=2).celestial;world=bool(panel.get('wcs_axes',params.get('wcs_axes',False)))
                if world and not wcs.has_celestial:raise ValueError('WCS axes requested for a panel without celestial WCS')
                ax=figure.add_subplot(rows,columns,index+1,projection=wcs if world else None)
                display_transform=rotated_view(ax,data,view)
                if world and float(view.get('rotation_deg',0.))%360:ax.reset_wcs(wcs=rotated_wcs(wcs,display_transform))
                pixels_transform=display_transform+ax.transData
                if world:
                    for coordinate,ctype in zip(ax.coords,wcs.wcs.ctype):
                        coordinate.set_axislabel('Right ascension' if ctype.startswith('RA') else 'Declination' if ctype.startswith('DEC') else 'Galactic longitude' if ctype.startswith('GLON') else 'Galactic latitude' if ctype.startswith('GLAT') else ctype,fontproperties=font)
                ax.imshow(pixels,origin='lower',interpolation='nearest',aspect='equal',transform=pixels_transform)
                if not world:
                    rotated=bool(float(view.get('rotation_deg',0.))%360)
                    ax.set_xlabel('rotated display x [pixels]' if rotated else 'x [zero-based pixels]',fontproperties=font);ax.set_ylabel('rotated display y [pixels]' if rotated else 'y [zero-based pixels]',fontproperties=font)
                ax.set_title(str(panel.get('title','')),fontproperties=font)
                for label in ax.get_xticklabels()+ax.get_yticklabels():label.set_fontproperties(font)
                for item in panel.get('annotations',view.get('annotations',[])):annotation(ax,item,font,wcs,pixels_transform)
                for aperture in panel.get('apertures',[]):
                    annotation(ax,dict(aperture['shape'],color='yellow'),font,wcs,pixels_transform)
                    if aperture.get('background'):annotation(ax,dict(aperture['background'],color='lime'),font,wcs,pixels_transform)
                if panel.get('colorbar',params.get('colorbar',True)):
                    position=panel.get('colorbar_position',params.get('colorbar_position','Right'));horizontal=position in ['Top','Bottom']
                    from matplotlib.colors import ListedColormap,Normalize
                    from matplotlib.cm import ScalarMappable
                    levels=np.linspace(low,high,1024).reshape(1,-1)
                    colors,_,_=rgba(levels,dict(view,low=low,high=high,render=dict(view.get('render',{}),mask=None)),histogram_cdf=cdf)
                    mappable=ScalarMappable(norm=Normalize(low,high),cmap=ListedColormap(colors[0]/255.))
                    bar=figure.colorbar(mappable,ax=ax,location=position.lower(),fraction=.05,pad=.04 if not horizontal else .10,shrink=.9)
                    bar.set_label(header.get('BUNIT','unit unspecified'),fontproperties=font)
                if panel.get('compass') or panel.get('scale_arcsec'):
                    if not wcs.has_celestial:raise ValueError('Compass/angle scale needs celestial WCS')
                    xlo,xhi=ax.get_xlim();ylo,yhi=ax.get_ylim();anchor=display_transform.inverted().transform([xlo+.15*(xhi-xlo),ylo+.15*(yhi-ylo)])
                    local=worker.wcs(dict(panel,points=[anchor]))['compass']
                    if panel.get('compass'):
                        for name,key in [('N','north'),('E','east')]:
                            v=np.asarray(local[key]);v=v/np.linalg.norm(v)*.08*min(xhi-xlo,yhi-ylo)
                            ax.annotate('',xy=np.asarray(anchor)+v,xytext=anchor,xycoords=pixels_transform,textcoords=pixels_transform,color='yellow',arrowprops={'arrowstyle':'->','color':'yellow'})
                            ax.text(*(np.asarray(anchor)+1.25*v),name,transform=pixels_transform,color='yellow',fontproperties=font,ha='center',va='center')
                    if panel.get('scale_arcsec'):
                        length=float(panel['scale_arcsec'])/local['arcsec_per_pixel_x'];base=display_transform.inverted().transform([xlo+.1*(xhi-xlo),ylo+.08*(yhi-ylo)])
                        ax.plot([base[0],base[0]+length],[base[1],base[1]],color='yellow',linewidth=2,transform=pixels_transform);ax.text(base[0],base[1],str(panel['scale_arcsec'])+' arcsec (local)',color='yellow',fontproperties=font,va='bottom',transform=pixels_transform)
                provenance.append({'path':str(panel['path']),'hdu':hdu,'view':view,'shape':list(data.shape),'bunit':header.get('BUNIT'),'wcs_axes':world,'annotations':panel.get('annotations',[]),'apertures':panel.get('apertures',[])})
            figure.savefig(path,format=format,dpi=dpi,transparent=bool(params.get('transparent',False)))
        record={'version':1,'pixel_size':[width,height],'dpi':dpi,'page_inches':[width/dpi,height/dpi],'format':format,'panels':provenance,'tex_engine_embedded':False,'math_backend':'user environment Matplotlib MathText subset','font':fontfile.name}
        sidecar=path.with_suffix(path.suffix+'.json');sidecar.write_text(json.dumps(clean(record),ensure_ascii=False,indent=2),encoding='utf-8')
        result=worker.artifact(path,format,width=width,height=height,dpi=dpi,vector_text_and_lines=format in ['pdf','svg']);result['provenance']=worker.artifact(sidecar,'json');return result
    finally:
        if figure is not None:figure.clear()

def rgb(worker,params):
    require('numpy','astropy','matplotlib','scipy')
    import numpy as np
    from astropy.wcs import WCS
    from astropy.wcs.utils import pixel_to_skycoord,skycoord_to_pixel
    from scipy.ndimage import map_coordinates
    from matplotlib import image as mpl_image
    channels=params['channels']
    if len(channels)!=3:raise ValueError('RGB needs exactly three channels')
    reference=worker.image(channels[0],data=True);shape=reference[3].shape
    if math.prod(shape)>16*1024*1024:raise ValueError('RGB reference grid exceeds 16 Mi pixels')
    target=WCS(reference[2],naxis=2).celestial;output=np.zeros((*shape,3),dtype='float32');valid=np.ones(shape,dtype=bool);reprojected=[]
    for i,channel in enumerate(channels):
        _,_,header,data=reference if i==0 else worker.image(channel,data=True)
        source=WCS(header,naxis=2).celestial;aligned=data.shape==shape
        if aligned and target.has_celestial and source.has_celestial:
            points=np.array([[0,0],[shape[1]-1,0],[0,shape[0]-1],[shape[1]-1,shape[0]-1],[(shape[1]-1)/2,(shape[0]-1)/2]])
            a=pixel_to_skycoord(points[:,0],points[:,1],target);b=pixel_to_skycoord(points[:,0],points[:,1],source);aligned=bool(np.all(a.separation(b).arcsec<.001))
        elif not params.get('assume_aligned',False):aligned=False
        if not aligned:
            if not params.get('reproject') or not target.has_celestial or not source.has_celestial:raise ValueError('Channels are not verified on the same grid. Enable WCS reprojection or explicitly confirm pixel alignment.')
            aligned_data=np.full(shape,np.nan)
            for row in range(0,shape[0],64):
                if params.get('_cancel') and params['_cancel'].is_set():raise InterruptedError('RGB cancelled')
                y,x=np.mgrid[row:min(row+64,shape[0]),:shape[1]];sky=pixel_to_skycoord(x,y,target);sx,sy=skycoord_to_pixel(sky,source);aligned_data[row:row+len(y)]=map_coordinates(data,[sy,sx],order=1,cval=np.nan,mode='constant',prefilter=False)
            data=aligned_data;reprojected.append(i)
        colors,_,_=rgba(data,dict(channels[i],render=dict(channels[i].get('render',{}),palette={'name':'Gray'},mask=None)));weight=float(channels[i].get('weight',1.))
        if not 0<=weight<=100:raise ValueError('RGB weight must be between zero and 100')
        output[:,:,i]=np.clip(colors[:,:,0]/255*weight,0,1);valid&=np.isfinite(data)
    output[~valid]=[95/255,45/255,110/255];path=worker.cache/('rgb-'+uuid.uuid4().hex+'.png');mpl_image.imsave(path,output,origin='lower')
    result=worker.artifact(path,'png',width=shape[1],height=shape[0],reprojected_channels=reprojected,reference_path=str(channels[0]['path']),reference_hdu=reference[1],photometry_channel='R reference data, never RGB display intensity');return result

def math_preview(worker,params):
    require('matplotlib')
    import matplotlib
    matplotlib.use('Agg',force=True)
    from matplotlib.mathtext import math_to_image
    expression=str(params['expression'])
    if len(expression)>4096:raise ValueError('Math expression exceeds 4096 characters')
    path=worker.cache/('math-'+uuid.uuid4().hex+'.svg')
    preview=path.with_suffix('.png')
    with matplotlib.rc_context({'text.usetex':False,'svg.fonttype':'path'}):
        math_to_image(expression,str(path),format='svg',dpi=150)
        math_to_image(expression,str(preview),format='png',dpi=150)
    result=worker.artifact(preview,'png',engine='user environment MathText subset',complete_latex=False,tex_engine_embedded=False)
    result['vector']=worker.artifact(path,'svg');return result

def request(worker,operation,params):return globals()[operation](worker,params)
