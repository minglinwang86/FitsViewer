"""Shared bounded VO/TAP transport for Gaia and PRO. / 共用有界 TAP 传输。"""
from pathlib import Path
import io,math,time,urllib.parse,urllib.request,uuid
from fitsviewer_science import require,clean,MAX_INPUT,MAX_TABLE
def http_url(value):
    parsed=urllib.parse.urlsplit(value)
    if parsed.scheme not in ['http','https'] or not parsed.hostname or parsed.username or parsed.password:raise ValueError('Use an HTTP(S) service URL without embedded credentials')
    return value

def query(worker,params):
    require('astropy')
    from astropy.io.votable import from_table,writeto
    protocol=params.get('protocol','tap').lower();url=http_url(params['service']);maximum=max(1,min(int(params.get('max_rows',1000)),20000));cancel=params.get('_cancel')
    started=time.time();job_url=None
    from fitsviewer_network import bounded_timeout,session as bounded_session,file_capability_request
    file_capability=file_capability_request(worker,params)
    timeout=bounded_timeout(params.get('timeout',120))
    session=bounded_session(cancel,timeout);transport={};table=None;results=None
    if protocol in ['tap','vizier']:
        adql=params.get('adql','')
        if not adql.strip() or len(adql)>32000:raise ValueError('Provide a nonempty ADQL query up to 32,000 characters')
        uploads={}
        for name,path in params.get('uploads',{}).items():
            from fitsviewer_science import bounded_path
            if not name.replace('_','').isalnum() or len(uploads)>=3:raise ValueError('At most three named TAP uploads')
            uploads[name]=str(bounded_path(path,MAX_TABLE))
        if file_capability and (params.get('async') or uploads):
            raise ValueError('File capabilities require bounded self-contained TAP responses; asynchronous jobs and uploads require a trusted session')
        if params.get('async'):
            require('pyvo')
            import pyvo
            service=pyvo.dal.TAPService(url,session=session)
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
                session.close()
        elif uploads:
            require('pyvo')
            import pyvo
            try:results=pyvo.dal.TAPService(url,session=session).run_sync(adql,maxrec=maximum,uploads=uploads)
            finally:session.close()
        else:
            from fitsviewer_network import fetch_bytes,parse_votable
            body,transport=fetch_bytes(url.rstrip('/')+'/sync',data={'REQUEST':'doQuery','LANG':'ADQL','FORMAT':'votable','MAXREC':maximum,'QUERY':adql},cancel=cancel,timeout=timeout)
            table,parsed=parse_votable(body,file_capability=file_capability);transport.update(parsed)
    elif protocol in ['sia','ssa','sia2']:
        from fitsviewer_network import fetch_bytes,parse_votable
        ra,dec=float(params['ra_deg']),float(params['dec_deg']);size=float(params.get('size_deg',.1))
        if not math.isfinite(ra) or not math.isfinite(dec) or not -90<=dec<=90 or not math.isfinite(size) or not 0<size<=10:raise ValueError('Provide finite sky coordinates and a positive field size up to 10 degrees')
        values={'POS':f'{ra%360:.12f},{dec:.12f}','SIZE':size,'FORMAT':params.get('format') or 'ALL'}
        if protocol=='ssa':values['REQUEST']='queryData'
        if protocol=='sia2':values={'POS':f'CIRCLE {ra%360:.12f} {dec:.12f} {size/2:.12f}','MAXREC':maximum}
        for key in ['collection','band']:
            if params.get(key):values[key.upper()]=str(params[key])[:256]
        body,transport=fetch_bytes(url,params=values,cancel=cancel,timeout=timeout)
        table,parsed=parse_votable(body,file_capability=file_capability);transport.update(parsed)
    else:raise ValueError('Protocol must be SIA, SIA2, SSA, TAP or VizieR TAP')
    if cancel and cancel.is_set():raise InterruptedError('VO query cancelled')
    session.close()
    if table is None:table=results.to_table()
    total=len(table);table=table[:maximum]
    buffer=io.BytesIO();writeto(from_table(table),buffer)
    if buffer.tell()>MAX_TABLE:raise ValueError('VO response exceeds the table cache limit')
    path=worker.cache/('vo-'+uuid.uuid4().hex+'.vot');path.write_bytes(buffer.getvalue())
    preview={name:clean(table[name][:100].tolist()) for name in table.colnames[:64]}
    result=worker.artifact(path,'votable',protocol=protocol,service=url,query=params.get('adql'),rows=len(table),reported_rows=total,
                           status='empty' if not len(table) else 'ok',truncated=total>maximum or transport.get('query_status')=='OVERFLOW' or str(getattr(results,'query_status','')).upper()=='OVERFLOW',preview=preview,columns=table.colnames,elapsed_seconds=time.time()-started,remote_job=job_url,transport=transport)
    result['download_url_columns']=[name for name in table.colnames if name.lower() in ['access_url','access_reference','acref','url','image_url','spectral_access_reference']]
    return result
