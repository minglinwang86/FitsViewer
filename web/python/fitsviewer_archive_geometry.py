"""Bounded spherical footprint checks; no planar RA/Dec area assumptions.

面积比是候选天区面积/当前图像天区面积，另需真实相交；未知几何不冒充合格。
"""
import math

DEG2=(180./math.pi)**2

def vectors(points):
    import numpy as np
    values=np.deg2rad(np.asarray(points,dtype=float).reshape(-1,2))
    return np.column_stack((np.cos(values[:,1])*np.cos(values[:,0]),np.cos(values[:,1])*np.sin(values[:,0]),np.sin(values[:,1])))

def coordinates(points):
    import numpy as np
    return np.column_stack((np.rad2deg(np.arctan2(points[:,1],points[:,0]))%360,np.rad2deg(np.arcsin(np.clip(points[:,2],-1,1))))).tolist()

def convex(points):
    import numpy as np
    if not 3<=len(points)<=1024 or not np.isfinite(points).all():return False
    center=points.sum(axis=0);length=np.linalg.norm(center)
    if length<1e-12:return False
    center/=length
    if np.any(points@center<=0):return False
    normals=np.cross(points,np.roll(points,-1,axis=0));lengths=np.linalg.norm(normals,axis=1)
    normals=normals[lengths>1e-15]/lengths[lengths>1e-15,None]
    if len(normals)<3:return False
    sign=normals@center
    if np.any(sign>1e-13) and np.any(sign< -1e-13):return False
    orientation=1 if sign.sum()>=0 else -1
    tangent=points-(points@center)[:,None]*center;following=np.roll(tangent,-1,axis=0)
    winding=np.arctan2(np.cross(tangent,following)@center,np.einsum('ij,ij->i',tangent,following)).sum()
    return bool(abs(abs(winding)-2*math.pi)<1e-8 and np.all(orientation*(normals@points.T)>=-1e-10))

def area(points):
    import numpy as np
    if len(points)<3:return 0.
    center=np.sum(points,axis=0);center/=np.linalg.norm(center)
    following=np.roll(points,-1,axis=0)
    numerator=np.einsum('ij,j->i',np.cross(points,following),center)
    denominator=1+points@center+following@center+np.einsum('ij,ij->i',points,following)
    return float(abs(2*np.arctan2(numerator,denominator).sum())*DEG2)

def intersection(first,second):
    import numpy as np
    output=list(first);center=np.sum(second,axis=0)
    for a,b in zip(second,np.roll(second,-1,axis=0)):
        normal=np.cross(a,b);length=np.linalg.norm(normal)
        if length<1e-15:continue
        normal/=length
        if normal@center<0:normal=-normal
        previous=output;output=[]
        if not previous:break
        for start,end in zip(previous[-1:]+previous[:-1],previous):
            ds,de=float(normal@start),float(normal@end);inside_s,inside_e=ds>=-1e-14,de>=-1e-14
            if inside_s!=inside_e:
                point=start+(end-start)*(ds/(ds-de));point/=np.linalg.norm(point);output.append(point)
            if inside_e:output.append(end)
    return np.asarray(output)

def components(points):
    """Accept a convex polygon or a simple star-shaped same-hemisphere boundary.

    WCS 畸变可让图像边轻微内凹；同向且仅绕中心一周的扇形三角形不重叠。
    不接受自交、多绕圈或无法确定内部的足迹。
    """
    import numpy as np
    if convex(points):return [points]
    if not 3<=len(points)<=1024 or not np.isfinite(points).all():return None
    center=points.sum(axis=0);length=np.linalg.norm(center)
    if length<1e-12:return None
    center/=length;dot=points@center
    if np.any(dot<=0):return None
    axis=np.array([1.,0,0]) if abs(center[0])<.8 else np.array([0.,1,0]);east=np.cross(center,axis);east/=np.linalg.norm(east);north=np.cross(center,east)
    plane=np.column_stack((points@east,points@north))/dot[:,None];following=np.roll(plane,-1,axis=0)
    if np.any(np.linalg.norm(plane,axis=1)<1e-15):return None
    turns=np.arctan2(plane[:,0]*following[:,1]-plane[:,1]*following[:,0],np.einsum('ij,ij->i',plane,following))
    if not (np.all(turns>=-1e-12) or np.all(turns<=1e-12)) or abs(abs(turns.sum())-2*math.pi)>1e-8:return None
    return [np.array([center,a,b]) for a,b,turn in zip(points,np.roll(points,-1,axis=0),turns) if abs(turn)>1e-14]


def overlap_area(shape,target):
    pieces=components(target)
    return None if pieces is None else sum(area(intersection(shape,piece)) for piece in pieces)

def sia_footprint(table,row):
    """Use a complete, declared SIA1 WCS; never invent an absent orientation.

    IVOA SIA1 的 CD 顺序为11,12,21,22，CRPIX使用FITS的一基坐标。
    缺投影/参考点/矩阵时仅保留未知几何，不套用默认北向来宣称面积合格。
    """
    import numpy as np
    import astropy.units as u
    from astropy.wcs import WCS
    from astropy.wcs.utils import pixel_to_skycoord
    from astropy.coordinates import SkyCoord
    columns={str(table[name].meta.get('ucd','')).lower():name for name in table.colnames}
    def value(ucd):
        name=columns.get(ucd.lower())
        if name is None or np.ma.is_masked(row[name]):return None
        result=row[name]
        return result.decode('utf-8','replace') if isinstance(result,bytes) else result
    def numbers(ucd,count):
        result=value(ucd)
        if isinstance(result,str):result=result.replace(',',' ').split()
        result=np.asarray(result,dtype=float).reshape(-1)
        if len(result)!=count or not np.isfinite(result).all():raise ValueError('Incomplete SIA WCS')
        return result
    def degrees(ucd):
        name=columns[ucd.lower()];unit=table[name].unit
        if not unit:return 1.
        try:return u.Unit(unit).to(u.deg)
        except u.UnitConversionError:return u.Unit(unit).to(u.deg/u.pix)
    try:
        if numbers('VOX:Image_Naxes',1)[0]!=2:return None
        dimensions=numbers('VOX:Image_Naxis',2)
        if np.any(dimensions<2) or np.any(dimensions>10**9) or np.any(dimensions%1):return None
        frame=str(value('VOX:STC_CoordRefFrame') or '').strip().upper();projection=str(value('VOX:WCS_CoordProjection') or '').strip().upper()
        if frame not in ('ICRS','FK5') or projection!='TAN':return None
        reference=numbers('VOX:WCS_CoordRefPixel',2);world=numbers('VOX:WCS_CoordRefValue',2)*degrees('VOX:WCS_CoordRefValue')
        cd=numbers('VOX:WCS_CDMatrix',4).reshape(2,2)*degrees('VOX:WCS_CDMatrix')
        if abs(world[1])>90 or abs(np.linalg.det(cd))<1e-24:return None
        header={'NAXIS':2,'NAXIS1':int(dimensions[0]),'NAXIS2':int(dimensions[1]),'CTYPE1':'RA---TAN','CTYPE2':'DEC--TAN','CUNIT1':'deg','CUNIT2':'deg',
                'RADESYS':frame,'CRPIX1':reference[0],'CRPIX2':reference[1],'CRVAL1':world[0],'CRVAL2':world[1],
                'CD1_1':cd[0,0],'CD1_2':cd[0,1],'CD2_1':cd[1,0],'CD2_2':cd[1,1]}
        if frame=='FK5':
            equinox=value('VOX:STC_CoordEquinox');header['EQUINOX']=2000. if equinox is None else float(equinox)
            if not math.isfinite(header['EQUINOX']):return None
        wcs=WCS(header);width,height=dimensions
        sky=pixel_to_skycoord([-.5,width-.5,width-.5,-.5],[-.5,-.5,height-.5,height-.5],wcs,origin=0).icrs
        points=np.column_stack((sky.ra.deg,sky.dec.deg));shape=vectors(points)
        if not convex(shape) or area(shape)<=0:return None
        ra=value('POS_EQ_RA_MAIN');dec=value('POS_EQ_DEC_MAIN')
        if ra is not None and dec is not None:
            center=pixel_to_skycoord((width-1)/2,(height-1)/2,wcs,origin=0).icrs
            if center.separation(SkyCoord(float(ra),float(dec),unit='deg',frame='icrs')).deg>max(1e-6,2*np.linalg.norm(cd,axis=0).max()):return None
        return {'footprint':'POLYGON ICRS '+' '.join(f'{v:.14g}' for point in points for v in point),
                'footprint_source':'provider-declared complete SIA1 TAN WCS (FITS 1-based CRPIX; CD order 11,12,21,22)',
                'footprint_header_verified':False,'footprint_wcs':header}
    except (ValueError,TypeError,KeyError,u.UnitConversionError):return None


def footprint(text):
    import numpy as np
    if not isinstance(text,str) or len(text)>128000:return None
    parts=text.split()
    if not parts:return None
    kind=parts.pop(0).upper();frame='icrs'
    if parts and parts[0].upper() in ('ICRS','FK5','J2000'):frame=parts.pop(0).lower()
    try:values=np.asarray([float(v) for v in parts])
    except ValueError:return None
    if not np.isfinite(values).all():return None
    if kind=='POLYGON' and 6<=len(values)<=2048 and not len(values)%2:points=values.reshape(-1,2);exact=None
    elif kind=='CIRCLE' and len(values)==3 and abs(values[1])<=90 and 0<values[2]<90:
        from astropy.coordinates import SkyCoord
        import astropy.units as u
        sky=SkyCoord(values[0]*u.deg,values[1]*u.deg,frame='icrs').directional_offset_by(np.linspace(0,360,257)[:-1]*u.deg,values[2]*u.deg)
        points=np.column_stack((sky.ra.deg,sky.dec.deg));exact=4*math.pi*math.sin(math.radians(values[2])/2)**2*DEG2
    else:return None
    if np.any(abs(points[:,1])>90):return None
    if frame!='icrs':
        from astropy.coordinates import SkyCoord,FK5
        from astropy.time import Time
        sky=SkyCoord(points[:,0],points[:,1],unit='deg',frame=FK5(equinox=Time('J2000'))).icrs
        points=np.column_stack((sky.ra.deg,sky.dec.deg))
    result=vectors(points)
    if not convex(result):return None
    return result,area(result) if exact is None else exact

def select_products(products,field):
    target=vectors(field.get('field_boundary_icrs',[]));target_area=field.get('field_area_sqdeg')
    counts={'outside':0,'over_150_percent':0,'unknown_geometry':0,'preferred':0,'partial':0};good=[];partial=[]
    for product in products:
        parsed=footprint(product.get('footprint'))
        if not target_area or not math.isfinite(target_area) or target_area<=0 or components(target) is None or parsed is None:
            product.update(area_selection='unverified',area_ratio=None);counts['unknown_geometry']+=1;continue
        shape,size=parsed;overlap=overlap_area(shape,target)
        ratio=size/target_area
        product.update(area_sqdeg=size,field_area_sqdeg=target_area,area_ratio=ratio,overlap_sqdeg=overlap,overlap_fraction=overlap/target_area,
                       area_definition='candidate spherical footprint area / current image spherical footprint area',
                       coverage_check='overlaps_current_image' if overlap>1e-12*target_area else 'outside_query',spatially_consistent=overlap>1e-12*target_area)
        if overlap<=1e-12*target_area:counts['outside']+=1;product['area_selection']='outside';continue
        if ratio>1.5+1e-9:counts['over_150_percent']+=1;product['area_selection']='oversized';continue
        if ratio>=.4-1e-9:good.append(product);counts['preferred']+=1;product['area_selection']='preferred_40_to_150_percent'
        else:partial.append(product);counts['partial']+=1;product['area_selection']='partial_fallback_below_40_percent'
    selected=good or partial
    selected.sort(key=lambda row:abs(row['area_ratio']-1))
    return selected,dict(counts,partial_fallback=not good and bool(partial),selection_scope='bounded returned products only; truncated results are not a whole-service completeness claim',
                         area_ratio_bounds=[.4,1.5],boundary_tolerance=1e-9)

def image_field(worker,params):
    import numpy as np
    from astropy.wcs.utils import pixel_to_skycoord
    _,_,header,_=worker.image(params,data=False);_,wcs=worker.celestial(params)
    width,height=int(header['NAXIS1']),int(header['NAXIS2'])
    if width<2 or height<2:raise ValueError('Current image must have a valid two-dimensional WCS field')
    steps=np.linspace(0,1,9)[:-1]
    pixels=np.vstack((np.column_stack((-.5+steps*width,np.full(8,-.5))),np.column_stack((np.full(8,width-.5),-.5+steps*height)),
                      np.column_stack((width-.5-steps*width,np.full(8,height-.5))),np.column_stack((np.full(8,-.5),height-.5-steps*height))))
    sky=pixel_to_skycoord(pixels[:,0],pixels[:,1],wcs,origin=0,mode='all').icrs
    boundary=np.column_stack((sky.ra.deg,sky.dec.deg));shape=vectors(boundary)
    center=pixel_to_skycoord((width-1)/2,(height-1)/2,wcs,origin=0,mode='all').icrs
    radius=float(center.separation(sky).deg.max())*1.000001
    if not np.isfinite(boundary).all() or not math.isfinite(radius) or not .000001<=radius<=1.5:raise ValueError('Current image WCS is missing/invalid or exceeds the supported 1.5 degree field radius; choose an explicit bounded field')
    pieces=components(shape)
    field={'ra_deg':float(center.ra.deg),'dec_deg':float(center.dec.deg),'radius_deg':radius,'field_boundary_icrs':boundary.tolist(),
           'field_area_sqdeg':area(shape) if pieces is not None else None,'field_convex':convex(shape),
           'field_geometry':'sampled WCS boundary, convex spherical polygon' if convex(shape) else 'sampled simple star-shaped WCS boundary; conservative discovery cone' if pieces is not None else 'conservative cone; complex field area unverified'}
    if pieces is None:return field
    # Fit a tangent-plane bounding parallelogram in the image's local pixel axes.
    # 线性 TAN 图精确保持足迹；畸变 WCS 用采样边界包络并重新检查面积比例。
    ra,dec=math.radians(field['ra_deg']),math.radians(field['dec_deg']);c=np.array([math.cos(dec)*math.cos(ra),math.cos(dec)*math.sin(ra),math.sin(dec)])
    east=np.array([-math.sin(ra),math.cos(ra),0.]);north=np.cross(c,east)
    local=pixel_to_skycoord([(width-1)/2+1,(width-1)/2],[(height-1)/2,(height-1)/2+1],wcs,origin=0,mode='all').icrs
    local=vectors(np.column_stack((local.ra.deg,local.dec.deg)));basis=np.vstack((local@east,local@north));basis/=np.linalg.norm(basis,axis=0)
    if abs(np.linalg.det(basis))<1e-6:return field
    plane=np.column_stack((shape@east,shape@north))/(shape@c)[:,None];axes=np.linalg.solve(basis,plane.T).T
    lo,hi=axes.min(axis=0),axes.max(axis=0);nx=max(16,round(512*width/max(width,height)));ny=max(16,round(512*height/max(width,height)))
    scale=(hi-lo)/[nx,ny];cd=basis*scale*180/math.pi
    cutout={'NAXIS':2,'NAXIS1':nx,'NAXIS2':ny,'CTYPE1':'RA---TAN','CTYPE2':'DEC--TAN','CUNIT1':'deg','CUNIT2':'deg','RADESYS':'ICRS',
            'CRVAL1':field['ra_deg'],'CRVAL2':field['dec_deg'],'CRPIX1':.5-lo[0]/scale[0],'CRPIX2':.5-lo[1]/scale[1],
            'CD1_1':float(cd[0,0]),'CD1_2':float(cd[0,1]),'CD2_1':float(cd[1,0]),'CD2_2':float(cd[1,1])}
    from astropy.wcs import WCS
    corners=pixel_to_skycoord([-.5,nx-.5,nx-.5,-.5],[-.5,-.5,ny-.5,ny-.5],WCS(cutout),origin=0).icrs
    output_shape=vectors(np.column_stack((corners.ra.deg,corners.dec.deg)));ratio=area(output_shape)/field['field_area_sqdeg']
    if .4-1e-9<=ratio<=1.5+1e-9:field.update(cutout_wcs=cutout,cutout_area_ratio=ratio,cutout_area_sqdeg=area(output_shape),cutout_overlap_fraction=overlap_area(output_shape,shape)/field['field_area_sqdeg'])
    return field
