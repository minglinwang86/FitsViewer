"""Optional VO/CDS client; no network request occurs before an explicit operation.
VO/CDS 客户端只按明确请求访问服务，外部依赖从用户环境导入。
"""
from pathlib import Path
import io
import math
import time
import urllib.parse
import urllib.request
import uuid
from fitsviewer_science import require,clean,MAX_INPUT,MAX_TABLE
from fitsviewer_tap import http_url,query



def download(worker,params):
    url=http_url(params['url']);maximum=min(int(params.get('maximum_bytes',MAX_INPUT)),MAX_INPUT);cancel=params.get('_cancel')
    suffix=Path(urllib.parse.urlsplit(url).path).suffix.lower()
    if suffix not in ['.fits','.fit','.fts','.fz','.gz','.txt','.dat','.csv','.vot','.xml']:suffix='.fits'
    target=worker.cache/('download-'+uuid.uuid4().hex+suffix);size=0
    try:
        with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'FitsViewer science adapter'}),timeout=30) as response,target.open('wb') as out:
            if int(response.headers.get('Content-Length','0'))>maximum:raise ValueError('Remote file exceeds the download limit')
            while block:=response.read(1024*1024):
                if cancel and cancel.is_set():raise InterruptedError('Download cancelled')
                size+=len(block)
                if size>maximum:raise ValueError('Download exceeded the limit')
                out.write(block)
        return worker.artifact(target,'fits' if suffix in ['.fits','.fit','.fts','.fz','.gz'] else 'download',source=url)
    except Exception:
        target.unlink(missing_ok=True);raise

def xmatch(worker,params):
    require('astroquery','astropy')
    from astroquery.xmatch import XMatch
    from astropy.table import Table
    from astropy.io.votable import from_table,writeto
    import astropy.units as u
    from fitsviewer_science import bounded_path
    XMatch.TIMEOUT=60
    if params.get('list_catalogues'):
        from fitsviewer_network import session as bounded_session
        previous=XMatch._session;session=bounded_session(params.get('_cancel'),params.get('timeout',120));XMatch._session=session
        try:return {'catalogues':XMatch.get_available_tables(cache=False)}
        finally:XMatch._session=previous;session.close()
    first=Table.read(bounded_path(params['catalogue'],MAX_TABLE))
    maximum=max(1,min(int(params.get('upload_rows',1000)),20000));first=first[:maximum]
    coordinates=worker.catalogue({key:value for key,value in params.items() if key not in ['path','hdu']})['coordinates']
    import numpy as np
    ra=np.asarray(coordinates['ra_deg'][:len(first)],dtype=float);dec=np.asarray(coordinates['dec_deg'][:len(first)],dtype=float);valid=np.isfinite(ra)&np.isfinite(dec)
    first['fv_source_row']=np.arange(len(first));first['fv_ra_j2000']=ra;first['fv_dec_j2000']=dec;first=first[valid]
    selected=params.get('upload_columns',[])
    if not isinstance(selected,list) or len(selected)>16 or any(not isinstance(name,str) or name not in first.colnames for name in selected):raise ValueError('Select at most 16 existing extra upload columns')
    upload_columns=list(dict.fromkeys(['fv_source_row','fv_ra_j2000','fv_dec_j2000',*selected]))
    first=first[upload_columns]
    radius=float(params.get('radius_arcsec',1.))
    if not 0<radius<=180:raise ValueError('CDS matching radius must be between zero and 180 arcsec')
    second=params['remote_catalogue']
    if not isinstance(second,str) or not second.startswith('vizier:'):raise ValueError('Choose a CDS VizieR catalogue identifier')
    from fitsviewer_network import session as bounded_session
    previous=XMatch._session;session=bounded_session(params.get('_cancel'),params.get('timeout',120));XMatch._session=session
    try:result=XMatch.query(cat1=first,cat2=second,max_distance=radius*u.arcsec,colRA1='fv_ra_j2000',colDec1='fv_dec_j2000',cache=False)
    finally:XMatch._session=previous;session.close()
    path=worker.cache/('xmatch-'+uuid.uuid4().hex+'.vot');writeto(from_table(result),str(path))
    return worker.artifact(path,'votable',rows=len(result),uploaded_rows=len(first),uploaded_columns=upload_columns,invalid_rows_excluded=int((~valid).sum()),upload_frame='FK5 J2000, decimal degrees',remote_catalogue=second,radius_arcsec=radius,geometric_only=True,preview={name:clean(result[name][:100].tolist()) for name in result.colnames[:64]})

def request(worker,operation,params):
    return {'vo_query':query,'vo_download':download,'xmatch_cds':xmatch}[operation](worker,params)
