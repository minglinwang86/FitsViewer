"""Optional VO/CDS client; no network request occurs before an explicit operation.
VO/CDS 客户端只按明确请求访问服务，外部依赖从用户环境导入。
"""
from pathlib import Path
import bz2
import gzip
import io
import json
import math
import time
import urllib.parse
import urllib.request
import uuid
from fitsviewer_science import require,clean,MAX_INPUT,MAX_TABLE
from fitsviewer_tap import http_url,query



def download(worker,params):
    from fitsviewer_network import bounded_timeout,ServiceError,file_capability_request
    if file_capability_request(worker,params):raise PermissionError('File-only remote sessions do not authorize downloading new resources; use a trusted full session')
    worker.check_cancelled()
    metadata={}
    if params.get('hips_cutout') is not None:
        if not isinstance(params['hips_cutout'],dict):raise ValueError('HiPS cutout settings must be an object')
        from fitsviewer_archive_services import hips_cutout
        settings=dict(params['hips_cutout'],_cancel=params.get('_cancel'),timeout=params.get('timeout',120))
        for key in ('path','hdu','wcs_override'):
            if key in params:settings[key]=params[key]
        url,metadata=hips_cutout(worker,settings)
    else:url=http_url(params['url'])
    requested=params.get('maximum_bytes',MAX_INPUT)
    if isinstance(requested,bool) or not isinstance(requested,int) or requested<1:raise ValueError('Download limit must be a positive integer number of bytes')
    maximum=min(requested,MAX_INPUT);cancel=params.get('_cancel');timeout=bounded_timeout(params.get('timeout',120))
    if 'datalink' in str(params.get('access_format','')).lower():raise ValueError('Resolve the DataLink response and select a product before downloading')
    token='download-'+uuid.uuid4().hex;target=worker.cache/(token+'.part');decoded=worker.cache/(token+'.decoded');size=0
    try:
        deadline=time.monotonic()+timeout
        with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'FitsViewer science adapter','Accept-Encoding':'identity'}),timeout=min(30.,timeout)) as response,target.open('wb') as out:
            if int(response.headers.get('Content-Length','0'))>maximum:raise ValueError('Remote file exceeds the download limit')
            declared=int(response.headers.get('Content-Length','0'));mime=response.headers.get('Content-Type','').lower();final_url=http_url(response.geturl())
            if 'datalink' in mime:raise ValueError('DataLink metadata must be resolved before selecting a download')
            if 'text/html' in mime:raise ServiceError('Download returned an HTML page, not a data product','protocol',url)
            while block:=response.read(1024*1024):
                if cancel and cancel.is_set():raise InterruptedError('Download cancelled')
                if time.monotonic()>deadline:raise TimeoutError('Download exceeded its deadline')
                size+=len(block)
                if size>maximum:raise ValueError('Download exceeded the limit')
                out.write(block)
        if size==0 or (declared and declared!=size):raise ValueError('Remote download is empty or shorter than its Content-Length')
        with target.open('rb') as stream:signature=stream.read(80)
        compression='gzip' if signature.startswith(b'\x1f\x8b') else 'bzip2' if signature.startswith(b'BZh') else None
        if compression:
            opener=gzip.open if compression=='gzip' else bz2.open;expanded=0
            with opener(target,'rb') as source,decoded.open('wb') as output:
                while block:=source.read(1024*1024):
                    if cancel and cancel.is_set():raise InterruptedError('Download decompression cancelled')
                    if time.monotonic()>deadline:raise TimeoutError('Download decompression exceeded its deadline')
                    expanded+=len(block)
                    if expanded>maximum:raise ValueError('Decompressed data exceeds the download limit')
                    output.write(block)
            target.unlink();target=decoded
            with target.open('rb') as stream:signature=stream.read(80)
        lower=signature.lstrip().lower()
        if lower.startswith((b'<!doctype html',b'<html')):raise ServiceError('Downloaded content is an HTML error page','protocol',url)
        if signature.startswith((b'SIMPLE  =',b'XTENSION=')):
            require('astropy')
            from astropy.io import fits
            with fits.open(target,memmap=False,lazy_load_hdus=True) as hdus:
                if not len(hdus):raise ValueError('Downloaded FITS contains no HDU')
                hdus.verify('exception')
                if metadata:
                    import numpy as np
                    from astropy.wcs import WCS
                    from astropy.wcs.utils import pixel_to_skycoord,skycoord_to_pixel
                    height,width=metadata['cutout']['height'],metadata['cutout']['width']
                    image=next((hdu for hdu in hdus if not isinstance(hdu,(fits.BinTableHDU,fits.TableHDU)) and tuple(hdu.shape or ())==(height,width)),None)
                    if image is None:raise ValueError('HiPS response is not the requested two-dimensional FITS cutout')
                    expected=WCS(json.loads(metadata['cutout']['wcs']))
                    x=np.array([-.5,width-.5,width-.5,-.5,(width-1)/2]);y=np.array([-.5,-.5,height-.5,height-.5,(height-1)/2])
                    sky=pixel_to_skycoord(x,y,expected,origin=0).icrs
                    actual=np.array(skycoord_to_pixel(sky,WCS(image.header).celestial,origin=0))
                    if not np.isfinite(actual).all() or np.max(abs(actual-[x,y]))>.01:raise ValueError('HiPS response WCS does not match the requested current-field footprint')
                    _,_,source_header,_=worker.image(params,data=False);_,source_wcs=worker.celestial(params)
                    inside=False
                    # A conservative MOC cone may touch only a corner outside a
                    # distorted image. Require real finite pixels inside its WCS.
                    # 保守查询不能冒充实图重叠；逐小块核对有限像素确在当前图像内。
                    for first in range(0,height,64):
                        if cancel and cancel.is_set():raise InterruptedError('Download cancelled during footprint validation')
                        yy,xx=np.nonzero(np.isfinite(image.data[first:first+64]))
                        if not len(xx):continue
                        sky=pixel_to_skycoord(xx,yy+first,WCS(image.header).celestial,origin=0).icrs
                        sx,sy=skycoord_to_pixel(sky,source_wcs,origin=0,mode='all')
                        if np.any(np.isfinite(sx)&np.isfinite(sy)&(sx>=-.5)&(sx<=source_header['NAXIS1']-.5)&(sy>=-.5)&(sy<=source_header['NAXIS2']-.5)):inside=True;break
                    if not inside:raise ValueError('HiPS cutout contains no finite image pixels overlapping the current image WCS footprint')
                    metadata.update(returned_wcs_verified=True,finite_pixels_overlap_current_image=True)
            suffix,kind='.fits','fits'
        elif b'<VOTABLE' in signature.upper() or ('votable' in mime):
            require('astropy')
            from fitsviewer_network import parse_votable,file_capability_request
            if target.stat().st_size>MAX_TABLE:raise ValueError('Downloaded VOTable exceeds the table limit')
            table,_=parse_votable(target.read_bytes(),file_capability=file_capability_request(worker,params))
            if {'access_url','semantics'}<=set(table.colnames):raise ValueError('Downloaded DataLink table must be resolved and a data product selected')
            suffix,kind='.vot','votable'
        elif mime.split(';')[0] in ['text/plain','text/csv','application/csv','text/tab-separated-values']:
            with target.open('rb') as stream:stream.read(min(maximum,65536)).decode('utf-8')
            suffix,kind=('.csv' if 'csv' in mime else '.txt'),'download'
        else:raise ServiceError('Downloaded content is not a supported FITS, VOTable or text product; archive/bundle downloads require an explicit archive client','protocol',url)
        if (metadata or params.get('expect_fits') is True) and kind!='fits':raise ServiceError('Selected image product did not return FITS data','protocol',url)
        worker.check_cancelled()
        final=worker.cache/(token+suffix);target.rename(final);target=final
        return worker.artifact(target,kind,source=url,resolved_url=final_url,content_type=mime,wire_bytes=size,compression=compression,validated_content=True,**metadata)
    except Exception:
        target.unlink(missing_ok=True);decoded.unlink(missing_ok=True);raise

def xmatch(worker,params):
    require('astroquery','astropy')
    from astroquery.xmatch import XMatchClass
    from astropy.table import Table
    from astropy.io.votable import from_table,writeto
    import astropy.units as u
    from fitsviewer_science import bounded_path
    from fitsviewer_network import read_table,file_capability_request
    client=XMatchClass();client.TIMEOUT=60
    if params.get('list_catalogues'):
        from fitsviewer_network import session as bounded_session
        client._session.close();session=bounded_session(params.get('_cancel'),params.get('timeout',120));client._session=session
        try:return {'catalogues':client.get_available_tables(cache=False)}
        finally:session.close()
    import numpy as np
    maximum=max(1,min(int(params.get('upload_rows',1000)),20000));coordinates=params.get('coordinates')
    if coordinates is not None:
        if not isinstance(coordinates,dict):raise ValueError('Coordinates must contain ra_deg and dec_deg arrays')
        from astropy.coordinates import SkyCoord,FK5
        from astropy.time import Time
        ra=np.asarray(coordinates.get('ra_deg'),dtype=float);dec=np.asarray(coordinates.get('dec_deg'),dtype=float)
        if ra.ndim!=1 or dec.ndim!=1 or len(ra)!=len(dec) or not 1<=len(ra)<=200000:raise ValueError('Coordinate columns must contain equal nonempty arrays up to 200,000 rows')
        frame=str(coordinates.get('frame','icrs')).lower()
        if frame not in ['icrs','fk5']:raise ValueError('Reuse coordinates in ICRS or FK5 J2000')
        finite=np.isfinite(ra)&np.isfinite(dec)&(np.abs(dec)<=90)
        sky=SkyCoord(np.where(finite,ra,np.nan),np.where(finite,dec,np.nan),unit='deg',frame=frame).transform_to(FK5(equinox=Time('J2000')))
        first=Table({'fv_source_row':np.arange(len(ra))})[:maximum];ra,dec=sky.ra.deg[:maximum],sky.dec.deg[:maximum]
    else:
        if not str(params.get('catalogue','')).strip():raise ValueError('Choose a local coordinate table or reuse current identified coordinates')
        if not str(params.get('ra_column','')).strip() or not str(params.get('dec_column','')).strip():raise ValueError('Choose both RA and Dec columns before CDS matching')
        first=read_table(bounded_path(params['catalogue'],MAX_TABLE),file_capability=file_capability_request(worker,params))[:maximum]
        missing=[name for name in [params['ra_column'],params['dec_column']] if name not in first.colnames]
        if missing:raise ValueError('Coordinate columns are absent: '+', '.join(missing))
        coordinates=worker.catalogue({key:value for key,value in params.items() if key not in ['path','hdu']})['coordinates']
        ra=np.asarray(coordinates['ra_deg'][:len(first)],dtype=float);dec=np.asarray(coordinates['dec_deg'][:len(first)],dtype=float)
    valid=np.isfinite(ra)&np.isfinite(dec)&(np.abs(dec)<=90)
    first['fv_source_row']=np.arange(len(first));first['fv_ra_j2000']=ra;first['fv_dec_j2000']=dec;first=first[valid]
    selected=params.get('upload_columns',[])
    if not isinstance(selected,list) or len(selected)>16 or any(not isinstance(name,str) or name not in first.colnames for name in selected):raise ValueError('Select at most 16 existing extra upload columns')
    upload_columns=list(dict.fromkeys(['fv_source_row','fv_ra_j2000','fv_dec_j2000',*selected]))
    first=first[upload_columns]
    if not len(first):raise ValueError('No finite coordinate rows are available to upload')
    radius=float(params.get('radius_arcsec',1.))
    if not 0<radius<=180:raise ValueError('CDS matching radius must be between zero and 180 arcsec')
    second=params['remote_catalogue']
    if not isinstance(second,str) or not second.startswith('vizier:'):raise ValueError('Choose a CDS VizieR catalogue identifier')
    from fitsviewer_network import session as bounded_session
    strategy=params.get('strategy','all')
    if strategy not in ['all','nearest']:raise ValueError('CDS selection strategy must be all or nearest')
    client._session.close();session=bounded_session(params.get('_cancel'),params.get('timeout',120));client._session=session
    try:result=client.query(cat1=first,cat2=second,max_distance=radius*u.arcsec,colRA1='fv_ra_j2000',colDec1='fv_dec_j2000',cache=False)
    finally:session.close()
    if not {'fv_source_row','angDist'}<=set(result.colnames):raise ValueError('CDS response lacks source rows or angular separation')
    if len(result):
        distance=np.asarray(result['angDist'],dtype=float)
        if not np.isfinite(distance).all() or np.any(distance<0) or np.any(distance>radius+1e-5):raise ValueError('CDS response contains invalid/out-of-radius match distances')
        result.sort(['fv_source_row','angDist'])
    rows=np.asarray(result['fv_source_row'],dtype=int);unique,counts=np.unique(rows,return_counts=True);countmap=dict(zip(unique.tolist(),counts.tolist()))
    ambiguous=[int(row) for row,count in countmap.items() if count>1];candidate_rows=len(result)
    result['fv_match_count']=[countmap[int(row)] for row in rows];result['fv_ambiguous']=[countmap[int(row)]>1 for row in rows]
    if strategy=='nearest' and len(result):result=result[np.r_[True,np.diff(rows)!=0]]
    path=worker.cache/('xmatch-'+uuid.uuid4().hex+'.vot');writeto(from_table(result),str(path))
    return worker.artifact(path,'votable',status='ok' if len(result) else 'empty',rows=len(result),candidate_rows=candidate_rows,matched_input_rows=len(unique),ambiguous_source_rows=ambiguous,
                           unmatched_source_rows=[int(row) for row in first['fv_source_row'] if int(row) not in countmap],strategy=strategy,one_to_one_global_assignment=False,
                           uploaded_rows=len(first),uploaded_columns=upload_columns,invalid_rows_excluded=int((~valid).sum()),upload_frame='FK5 J2000, decimal degrees',remote_catalogue=second,radius_arcsec=radius,geometric_only=True,
                           scientific_note='Multiple candidates remain ambiguous, including nearest selection; catalogue matching is positional, not a unique physical identification.',
                           columns=result.colnames,preview={name:clean(result[name][:100].tolist()) for name in result.colnames[:64]})

def request(worker,operation,params):
    return {'vo_query':query,'vo_download':download,'xmatch_cds':xmatch}[operation](worker,params)
