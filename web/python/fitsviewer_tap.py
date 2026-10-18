"""Shared bounded VO/TAP transport for Gaia and PRO. / 共用有界 TAP 传输。"""
from pathlib import Path
import io,math,time,urllib.parse,urllib.request,uuid
from fitsviewer_science import require,clean,MAX_INPUT,MAX_TABLE
def http_url(value):
    parsed=urllib.parse.urlsplit(value)
    if parsed.scheme not in ['http','https'] or not parsed.hostname or parsed.username or parsed.password:raise ValueError('Use an HTTP(S) service URL without embedded credentials')
    return value

def query(worker,params):
    require('pyvo','astropy')
    import pyvo
    import requests
    from astropy.io.votable import from_table,writeto
    protocol=params.get('protocol','tap').lower();url=http_url(params['service']);maximum=max(1,min(int(params.get('max_rows',1000)),20000));cancel=params.get('_cancel')
    started=time.time();job_url=None
    from fitsviewer_network import bounded_timeout,session as bounded_session
    timeout=bounded_timeout(params.get('timeout',120))
    session=bounded_session(cancel,timeout)
    if protocol in ['tap','vizier']:
        adql=params.get('adql','')
        if not adql.strip() or len(adql)>32000:raise ValueError('Provide a nonempty ADQL query up to 32,000 characters')
        service=pyvo.dal.TAPService(url,session=session)
        uploads={}
        for name,path in params.get('uploads',{}).items():
            from fitsviewer_science import bounded_path
            if not name.replace('_','').isalnum() or len(uploads)>=3:raise ValueError('At most three named TAP uploads')
            uploads[name]=str(bounded_path(path,MAX_TABLE))
        if params.get('async'):
            if cancel and cancel.is_set():raise InterruptedError('TAP job cancelled before submission')
            job=None;session.cancel=None
            try:
                job=service.submit_job(adql,maxrec=maximum,uploads=uploads or None);job_url=job.url;session.cancel=cancel
                job.run();deadline=time.monotonic()+timeout
                while True:
                    if cancel and cancel.is_set():raise InterruptedError('TAP job cancelled')
                    phase=job.phase
                    if phase=='COMPLETED':break
                    if phase in ['ERROR','ABORTED']:job.raise_if_error();raise RuntimeError('TAP job '+phase)
                    if time.monotonic()>deadline:raise TimeoutError('TAP job timed out')
                    time.sleep(.3)
                results=job.fetch_result()
            except BaseException:
                session.cancel=None;session.deadline=time.monotonic()+15
                try:
                    if job is not None:job.abort()
                    elif session.created_job_url:session.post(session.created_job_url+'/phase',data={'PHASE':'ABORT'})
                except Exception:pass
                raise
            finally:
                session.cancel=None;session.deadline=time.monotonic()+15
                try:
                    if job is not None:job.delete()
                    elif session.created_job_url:session.delete(session.created_job_url)
                except Exception:pass
        else:results=service.run_sync(adql,maxrec=maximum,uploads=uploads or None)
    elif protocol=='sia':
        results=pyvo.dal.SIAService(url,session=session).search(pos=(float(params['ra_deg']),float(params['dec_deg'])),size=float(params.get('size_deg',.1)),format=params.get('format'))
    elif protocol=='ssa':
        results=pyvo.dal.SSAService(url,session=session).search(pos=(float(params['ra_deg']),float(params['dec_deg'])),diameter=float(params.get('size_deg',.1)),format=params.get('format'))
    else:raise ValueError('Protocol must be SIA, SSA, TAP or VizieR TAP')
    if cancel and cancel.is_set():raise InterruptedError('VO query cancelled')
    table=results.to_table();total=len(table);table=table[:maximum]
    buffer=io.BytesIO();writeto(from_table(table),buffer)
    if buffer.tell()>MAX_TABLE:raise ValueError('VO response exceeds the table cache limit')
    path=worker.cache/('vo-'+uuid.uuid4().hex+'.vot');path.write_bytes(buffer.getvalue())
    preview={name:clean(table[name][:100].tolist()) for name in table.colnames[:64]}
    result=worker.artifact(path,'votable',protocol=protocol,service=url,query=params.get('adql'),rows=len(table),reported_rows=total,
                           truncated=total>maximum or str(getattr(results,'query_status','')).upper()=='OVERFLOW',preview=preview,columns=table.colnames,elapsed_seconds=time.time()-started,remote_job=job_url)
    result['download_url_columns']=[name for name in table.colnames if name.lower() in ['access_url','access_reference','acref','url','image_url','spectral_access_reference']]
    return result
