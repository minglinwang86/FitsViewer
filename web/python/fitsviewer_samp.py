"""Local SAMP Hub client using Astropy from the selected environment.
复用本机 Hub；远程浏览器通过单独的本机桥接命令接入，不捆绑 Java/Hub。
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import threading
import urllib.parse
import uuid
from fitsviewer_science import require,bounded_path,clean

MTYPES=['image.load.fits','table.load.votable','table.load.fits','coord.pointAt.sky','table.highlight.row','table.select.rowList','spectrum.load.ssa-generic']

def file_url(worker,path):
    path=bounded_path(path)
    if not getattr(worker,'samp_files',None):worker.samp_files={}
    identifier=uuid.uuid4().hex;worker.samp_files[identifier]=path
    if getattr(worker,'samp_http',None) is None:
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_GET(self):
                key=urllib.parse.urlsplit(self.path).path.strip('/');file=worker.samp_files.get(key)
                if file is None or not file.is_file():self.send_error(404);return
                self.send_response(200);self.send_header('Content-Type','application/octet-stream');self.send_header('Content-Length',str(file.stat().st_size));self.send_header('Cache-Control','no-store');self.end_headers()
                try:
                    with file.open('rb') as source:
                        while block:=source.read(1024*1024):self.wfile.write(block)
                except (BrokenPipeError,ConnectionResetError):pass
        worker.samp_http=ThreadingHTTPServer(('127.0.0.1',0),Handler)
        threading.Thread(target=worker.samp_http.serve_forever,daemon=True,name='fitsviewer-samp-files').start()
    return f'http://127.0.0.1:{worker.samp_http.server_port}/{identifier}'

def enqueue(worker,sender,mtype,params):
    sequence=getattr(worker,'samp_sequence',0)+1;worker.samp_sequence=sequence
    worker.events.append({'sequence':sequence,'sender':sender,'mtype':mtype,'params':clean(params)})

def disconnect(worker):
    """Release every local resource even if Hub disconnect fails. / 单步失败仍释放其余资源。"""
    error=None;client=worker.samp;worker.samp=None
    try:
        if client:client.disconnect()
    except Exception as failure:error=failure
    http=getattr(worker,'samp_http',None);worker.samp_http=None
    if http:
        try:http.shutdown()
        except Exception as failure:
            if error is None:error=failure
        try:http.server_close()
        except Exception as failure:
            if error is None:error=failure
    getattr(worker,'samp_files',{}).clear()
    getattr(worker,'samp_owners',set()).clear()
    worker.events.clear()
    if error is not None:raise error

def release_owner(worker,owner,max_document_generation=None,max_role_epoch=None):
    """Keep a shared connection until its final owner leaves. / 最后一个文档所有者离开才断开。"""
    owners=getattr(worker,'samp_owners',set())
    owners.difference_update([token for token in owners if worker.owner_matches(token,owner,max_document_generation,'pro' if max_role_epoch is not None else None,max_role_epoch)])
    if not owners:disconnect(worker)

def request(worker,params):
    action=params.get('action','status')
    owner=getattr(worker.scope,'owner','cli');document=getattr(worker.scope,'document_generation',0)
    epoch=getattr(worker.scope,'role_epoch',None)
    token=(owner,document,'pro',epoch or 0)
    if action=='disconnect':release_owner(worker,owner,document,epoch);return {'connected':worker.samp is not None}
    if action=='events':
        after=int(params.get('after',0));return {'events':[e for e in worker.events if e['sequence']>after],'latest':getattr(worker,'samp_sequence',0)}
    if action=='connect':
        require('astropy')
        from astropy.samp import SAMPIntegratedClient
        client=worker.samp
        if client is not None:
            worker.samp_owners.add(token)
            return {'connected':True,'client_id':client.get_public_id(),'profile':'standard','hub_location':'same machine as selected Python','subscriptions':MTYPES}
        client=SAMPIntegratedClient(addr='127.0.0.1',name='FitsViewer',description='FITS images, tables and coordinate inspection')
        try:client.connect()
        except BaseException:
            try:client.disconnect()
            except Exception:pass
            raise
        worker.samp=client;worker.samp_owners.add(token)
        def notification(private_key,sender,mtype,values,extra):enqueue(worker,sender,mtype,values)
        def call(private_key,sender,msg_id,mtype,values,extra):
            try:
                if mtype not in MTYPES:raise ValueError('Unsupported SAMP message')
                enqueue(worker,sender,mtype,values);client.reply(msg_id,{'samp.status':'samp.ok','samp.result':{}})
            except Exception as error:client.reply(msg_id,{'samp.status':'samp.error','samp.error':{'samp.errortxt':str(error)}})
        try:
            for mtype in MTYPES:
                client.bind_receive_notification(mtype,notification);client.bind_receive_call(mtype,call)
        except BaseException:
            try:disconnect(worker)
            except Exception:pass
            raise
        return {'connected':True,'client_id':client.get_public_id(),'profile':'standard','hub_location':'same machine as selected Python','subscriptions':MTYPES}
    client=worker.samp
    if client is None:return {'connected':False,'hint':'Start a local Hub such as TOPCAT/Aladin, or run the local bridge for remote Jupyter.'}
    if action=='status':
        peers=[]
        for identifier in client.get_registered_clients():
            try:peers.append({'id':identifier,'metadata':client.get_metadata(identifier),'subscriptions':list(client.get_subscriptions(identifier))})
            except Exception:pass
        return {'connected':True,'client_id':client.get_public_id(),'peers':peers,'subscriptions':MTYPES}
    if action=='send':
        mtype=params['mtype']
        if mtype not in MTYPES:raise ValueError('Unsupported SAMP message type')
        worker.samp_owners.add(token)
        values=dict(params.get('params',{}))
        if params.get('path'):values['url']=file_url(worker,params['path'])
        message={'samp.mtype':mtype,'samp.params':values}
        if params.get('recipient'):client.notify(params['recipient'],message);recipients=[params['recipient']]
        else:recipients=client.notify_all(message)
        return {'sent':True,'mtype':mtype,'recipients':recipients}
    raise ValueError('SAMP action must be connect/status/events/send/disconnect')
