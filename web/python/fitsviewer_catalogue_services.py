"""Catalogue identity overlays, separate from local intensity-peak detection.

星表投影沿当前图像的同一 WCS；不把亮点候选或最近邻几何关系称为物理认证。
"""
import math
from fitsviewer_science import require,clean

CDS_PRESETS=[
    {'id':'vizier:I/355/gaiadr3','label':'Gaia DR3'},
    {'id':'vizier:II/246/out','label':'2MASS point sources'},
    {'id':'vizier:II/349/ps1','label':'Pan-STARRS1'},
    {'id':'vizier:V/154/sdss16','label':'SDSS DR16 (multiple epochs possible)'},
    {'id':'vizier:I/345/gaia2','label':'Gaia DR2'},
]
CATALOGUE_PRESETS=[{'id':'gaia_dr3','label':'Gaia DR3','band':'G'},
                   {'id':'sdss_dr17','label':'SDSS DR17 primary stars','band':'r'}]


def identify(worker,params):
    require('numpy','astropy')
    import numpy as np
    from astropy.coordinates import FK5
    from astropy.time import Time
    from astropy.wcs.utils import pixel_to_skycoord,skycoord_to_pixel
    from fitsviewer_gaia import reference_table,reference_sky
    from fitsviewer_network import ServiceError,bounded_timeout,session
    header,wcs=worker.celestial(params)
    width,height=int(header['NAXIS1']),int(header['NAXIS2'])
    center=pixel_to_skycoord((width-1)/2,(height-1)/2,wcs,origin=0,mode='all').icrs
    corners=pixel_to_skycoord([0,width-1,width-1,0],[0,0,height-1,height-1],wcs,origin=0,mode='all')
    radius=max(float(center.separation(corners).deg.max())*1.02,.0001)
    if not math.isfinite(radius) or not 0<radius<=1.5:raise ValueError('Catalogue identification needs a usable celestial WCS with field radius up to 1.5 degrees')
    back=np.asarray(skycoord_to_pixel(center,wcs,origin=0,mode='all'),dtype=float)
    if not np.isfinite(back).all() or np.linalg.norm(back-[(width-1)/2,(height-1)/2])>.01:raise ValueError('Celestial WCS does not pass its center round-trip check')
    limit=max(1,min(int(params.get('max_rows',1000)),5000));magnitude=float(params.get('magnitude_limit',20.))
    if not math.isfinite(magnitude) or not 5<=magnitude<=22:raise ValueError('Magnitude limit must be between 5 and 22')
    source=params.get('catalogue_source','gaia_dr3');artifact=None
    if source=='gaia_dr3':
        table,label,artifact=reference_table(worker,center,radius,dict(params,reference_limit=limit,magnitude_limit=magnitude))
        id_column,mag_column,band='source_id','phot_g_mean_mag','G'
        if not {'ra','dec',id_column,mag_column}<=set(table.colnames):raise ServiceError('Gaia result is missing identity, position or magnitude columns','protocol')
    elif source=='sdss_dr17':
        require('astroquery')
        from astroquery.sdss import SDSSClass
        from astropy.table import Table
        import requests
        query=(f'SELECT TOP {limit} p.objID,p.ra,p.dec,p.psfMag_r FROM PhotoPrimary AS p '
               f'JOIN dbo.fGetNearbyObjEq({center.ra.deg:.12f},{center.dec.deg:.12f},{radius*60:.12f}) AS n ON p.objID=n.objID '
               f'WHERE p.type=6 AND p.psfMag_r<{magnitude:.8f} ORDER BY p.psfMag_r ASC')
        for attempt in range(2):
            client=SDSSClass();client._session.close();client._session=session(params.get('_cancel'),params.get('timeout',60))
            try:
                # A fresh class/session avoids global mutation and stale deadlines.
                # 公共查询只发送天区；不上传 FITS 或本地坐标表。
                table=client.query_sql(query,data_release=17,timeout=bounded_timeout(params.get('timeout',60)),cache=False)
                if table is None:table=Table(names=['objID','ra','dec','psfMag_r'],dtype=['U32','f8','f8','f8'])
                break
            except (requests.Timeout,TimeoutError) as error:
                if attempt:raise ServiceError('SDSS catalogue timed out after one retry: '+str(error),'network') from error
            except Exception as error:
                if isinstance(error,(InterruptedError,ModuleNotFoundError)):raise
                raise ServiceError('SDSS catalogue query failed: '+str(error),'protocol') from error
            finally:client._session.close()
        id_column,mag_column,band,label='objID','psfMag_r','r','SDSS DR17 / PhotoPrimary stars'
    else:raise ValueError('Catalogue source must be gaia_dr3 or sdss_dr17')
    worker.check_cancelled()
    if not {'ra','dec',id_column,mag_column}<=set(table.colnames):raise ServiceError('Catalogue result is missing identity, position or magnitude columns','protocol')
    valid=np.isfinite(np.ma.asarray(table['ra'],dtype=float).filled(np.nan))&np.isfinite(np.ma.asarray(table['dec'],dtype=float).filled(np.nan))
    valid&=np.abs(np.ma.asarray(table['dec'],dtype=float).filled(np.nan))<=90
    original_rows=len(table);table=table[valid]
    sky,epoch_note=reference_sky(table,header,params)
    x,y=skycoord_to_pixel(sky,wcs,origin=0,mode='all');j2000=sky.transform_to(FK5(equinox=Time('J2000')))
    visible=np.flatnonzero(np.isfinite(x)&np.isfinite(y)&(x>=-.5)&(x<width-.5)&(y>=-.5)&(y<height-.5))
    markers=[]
    for i in visible[:limit]:
        value=table[mag_column][i];mag=None if np.ma.is_masked(value) else float(value)
        if mag is not None and not math.isfinite(mag):mag=None
        markers.append({'row':int(i),'x':float(x[i]),'y':float(y[i]),'id':str(table[id_column][i]),
                        'ra_deg':float(j2000.ra.deg[i]),'dec_deg':float(j2000.dec.deg[i]),'magnitude':mag,'band':band,'source':label})
    result={'status':'ok' if markers else 'empty','catalogue_source':source,'source':label,'rows':original_rows,
            'visible_rows':len(visible),'markers':markers,'coordinate_origin':0,'frame':'FK5','equinox':'J2000',
            'coordinates':{'ra_deg':[m['ra_deg'] for m in markers],'dec_deg':[m['dec_deg'] for m in markers],'frame':'fk5','equinox':'J2000'},
            'wcs_source':'current Gaia/selected WCS override' if params.get('wcs_override') or params.get('view',{}).get('wcs_override') else 'FITS celestial WCS',
            'epoch_note':epoch_note,'invalid_rows_excluded':int((~valid).sum()),'truncated':len(visible)>limit or original_rows>=limit,
            'scientific_note':'Catalogue identities projected using the current WCS; individual detections and photometric calibration are not established by this overlay.'}
    if artifact:result['reference_artifact']=artifact
    return clean(result)
