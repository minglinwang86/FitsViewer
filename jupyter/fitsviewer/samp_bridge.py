"""Optional local SAMP bridge over an existing SSH-forwarded viewer HTTP endpoint.
本机桥接器复用已有 HTTP/SSH 转发；Astropy 与 Hub 均由用户环境提供。
"""
import argparse,collections,json,os,sys,tempfile,time,uuid
from pathlib import Path
import urllib.parse,urllib.request
from .runtime import resources

def command(arguments):
    parser=argparse.ArgumentParser(prog='fitsviewer samp-bridge')
    parser.add_argument('--url',required=True,help='Forwarded viewer base URL, including any JupyterHub prefix and /fitsviewer/')
    parser.add_argument('--token',default=os.environ.get('FITSVIEWER_BRIDGE_TOKEN',''),help='Viewer token or Jupyter API token; environment variable is preferred')
    parser.add_argument('--poll',type=float,default=1.)
    parser.add_argument('--once',action='store_true',help='Connect, process one poll, then clean up (diagnostic)')
    args=parser.parse_args(arguments[1:])
    parsed=urllib.parse.urlsplit(args.url)
    if parsed.scheme not in ['http','https'] or not parsed.hostname:raise ValueError('Use an HTTP(S) viewer URL')
    # Exact prefix is preserved. / 保留 JupyterHub 和反向代理的完整路径前缀。
    token=args.token or urllib.parse.parse_qs(parsed.query).get('token',[''])[0]
    base=urllib.parse.urlunsplit((parsed.scheme,parsed.netloc,parsed.path.rstrip('/')+'/', '', ''))
    args.token=token
    headers={'Authorization':'token '+args.token,'X-Fitsviewer-Token':args.token} if args.token else {}
    _,_,web=resources();sys.path.insert(0,str(web/'python'))
    from fitsviewer_science import Worker
    from fitsviewer_samp import request as samp,file_url
    owner='bridge-'+uuid.uuid4().hex;sequence=0;received=0
    def api(operation,params):
        body=json.dumps({'operation':operation,'params':dict(params,_viewer_session=owner)},ensure_ascii=False).encode('utf-8')
        request=urllib.request.Request(urllib.parse.urljoin(base,'api/science'),body,dict(headers,**{'Content-Type':'application/json'}))
        with urllib.request.urlopen(request,timeout=35) as response:return json.load(response)
    def transfer(url,target):
        request=urllib.request.Request(url,headers=headers if url.startswith(base) else {})
        with urllib.request.urlopen(request,timeout=30) as response,target.open('wb') as out:
            size=0
            while block:=response.read(512*1024):
                size+=len(block)
                if size>64*1024*1024:raise ValueError('SAMP transfer exceeds 64 MiB')
                out.write(block)
        return target
    with tempfile.TemporaryDirectory(prefix='fitsviewer-samp-') as directory:
        worker=Worker(directory)
        try:
            connection=samp(worker,{'action':'connect'})
            print(json.dumps({'connected':True,'profile':'standard','client_id':connection['client_id'],'remote_prefix':urllib.parse.urlsplit(base).path,'transfer_limit_mib':64}),flush=True)
            while True:
                api('bridge_heartbeat',{})
                events=api('bridge_poll',{'after':sequence});sequence=events.get('latest',sequence)
                for event in events.get('events',[]):
                    values=dict(event.get('params') or {});path=event.get('path')
                    if path:
                        suffix=Path(str(path)).suffix
                        if suffix not in ['.fits','.fz','.gz','.fit','.fts','.vot','.xml','.csv','.dat','.txt']:suffix='.fits'
                        local=Path(directory)/('remote-'+uuid.uuid4().hex+suffix)
                        transfer(urllib.parse.urljoin(base,'api/file')+'?'+urllib.parse.urlencode({'path':path}),local)
                        values['url']=file_url(worker,local)
                    samp(worker,{'action':'send','mtype':event['mtype'],'params':values,'recipient':event.get('recipient')})
                incoming=samp(worker,{'action':'events','after':received});received=incoming['latest']
                for event in incoming['events']:
                    values=dict(event['params']);path=None
                    if event['mtype'] in ['image.load.fits','table.load.fits','table.load.votable','spectrum.load.ssa-generic'] and values.get('url'):
                        url=values['url'];parts=urllib.parse.urlsplit(url);suffix=Path(parts.path).suffix.lower()
                        if suffix not in ['.fits','.fz','.gz','.fit','.fts','.vot','.xml','.csv','.dat','.txt']:suffix='.vot' if event['mtype']=='table.load.votable' else '.fits'
                        if parts.scheme=='file':
                            if parts.netloc not in ['', 'localhost']:raise ValueError('Only local file URLs are accepted from the local Hub')
                            local=Path(urllib.request.url2pathname(parts.path)).resolve(strict=True)
                            if local.stat().st_size>64*1024*1024:raise ValueError('SAMP transfer exceeds 64 MiB')
                        elif parts.scheme in ['http','https']:
                            local=transfer(url,Path(directory)/('hub-'+uuid.uuid4().hex+suffix))
                        else:raise ValueError('SAMP resource must use file or HTTP(S)')
                        upload=urllib.request.Request(urllib.parse.urljoin(base,'api/bridge/upload')+'?'+urllib.parse.urlencode({'suffix':suffix,'owner':owner}),local.read_bytes(),dict(headers,**{'Content-Type':'application/octet-stream'}))
                        with urllib.request.urlopen(upload,timeout=45) as response:path=json.load(response)['path']
                        values.pop('url',None)
                    api('bridge_publish',{'mtype':event['mtype'],'params':values,'path':path})
                if args.once:break
                time.sleep(max(.2,min(args.poll,5.)))
        except KeyboardInterrupt:pass
        finally:
            worker.close()
            try:api('release_view',{})
            except Exception:pass
    return 0

