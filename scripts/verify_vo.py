"""VO protocol fixtures and optional small public-service check; no catalogue database is bundled."""
import argparse,io,json,sys,threading,urllib.parse
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);parser.add_argument('--live',action='store_true');args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parents[1];sys.path.insert(0,str(root/'web/python'))
from fitsviewer_science import Worker,clean
from astropy.table import Table
from astropy.io.votable import from_table,writeto
requests=[];phase='PENDING';hold=False;report={'checks':[]};worker=Worker(args.output/'cache')
def votable(empty=False,error=False,overflow=False):
    table=Table({'ra':[1.,2.,3.],'dec':[4.,5.,6.],'access_url':['https://example.invalid/image.fits']*3})
    if empty:table=table[:0]
    stream=io.BytesIO();writeto(from_table(table),stream);data=stream.getvalue()
    status=b'<INFO name="QUERY_STATUS" value="'+(b'ERROR' if error else b'OVERFLOW' if overflow else b'OK')+b'">'+(b'fixture service error' if error else b'')+b'</INFO>'
    return data.replace(b'<RESOURCE type="results">',b'<RESOURCE type="results">'+status).replace(b'<RESOURCE type="results" ID="results">',b'<RESOURCE type="results" ID="results">'+status) if b'<RESOURCE type="results"' in data else data.replace(b'<RESOURCE type="results"/>',b'<RESOURCE type="results">'+status+b'</RESOURCE>')
class Handler(BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def respond(self,body,content='application/x-votable+xml',status=200):
        self.send_response(status);self.send_header('Content-Type',content);self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
    def body(self):return self.rfile.read(int(self.headers.get('Content-Length',0))).decode()
    def do_POST(self):
        global phase
        body=self.body();requests.append({'method':'POST','path':self.path,'body':body});values=urllib.parse.parse_qs(body);query=values.get('QUERY',[''])[0]
        if self.path=='/tap/async':
            phase='PENDING';self.send_response(303);self.send_header('Location',base+'/tap/async/1');self.end_headers()
        elif self.path.endswith('/phase'):
            phase='ABORTED' if 'ABORT' in body else 'EXECUTING' if hold else 'COMPLETED';self.respond(b'',content='text/plain')
        else:self.respond(votable(empty='empty' in query,error='error' in query,overflow='overflow' in query))
    def do_GET(self):
        requests.append({'method':'GET','path':self.path});path=urllib.parse.urlsplit(self.path).path
        if path=='/tap/async/1':
            body=f'''<?xml version="1.0"?><uws:job xmlns:uws="http://www.ivoa.net/xml/UWS/v1.0" xmlns:xlink="http://www.w3.org/1999/xlink"><uws:jobId>1</uws:jobId><uws:phase>{phase}</uws:phase><uws:executionDuration>120</uws:executionDuration><uws:results><uws:result id="result" xlink:href="{base}/tap/result"/></uws:results></uws:job>'''.encode();self.respond(body,'application/xml')
        elif path.endswith('/phase'):self.respond(phase.encode(),'text/plain')
        else:self.respond(votable())
    def do_DELETE(self):
        requests.append({'method':'DELETE','path':self.path});self.respond(b'',status=200)
server=ThreadingHTTPServer(('127.0.0.1',0),Handler);base=f'http://127.0.0.1:{server.server_port}';threading.Thread(target=server.serve_forever,daemon=True).start()
def check(name,function):
    try:result=function();report['checks'].append({'name':name,'passed':True,'result':clean(result)})
    except Exception as error:
        import traceback;traceback.print_exc();report['checks'].append({'name':name,'passed':False,'error':str(error)})
def query(protocol='tap',**extra):
    return worker.dispatch('vo_query',dict(service=base+('/tap' if protocol=='tap' else '/'+protocol),protocol=protocol,adql='SELECT fixture',ra_deg=1.,dec_deg=2.,max_rows=2,**extra))
try:
    check('tap_sync_truncation',lambda: query())
    check('sia_v1_fixed_response',lambda: query('sia'))
    check('ssa_fixed_response',lambda: query('ssa'))
    check('tap_empty',lambda: query(adql='SELECT empty') if False else worker.dispatch('vo_query',{'service':base+'/tap','protocol':'tap','adql':'SELECT empty'}))
    def error_test():
        try:worker.dispatch('vo_query',{'service':base+'/tap','protocol':'tap','adql':'SELECT error'})
        except Exception as error:return {'expected_error':str(error)}
        raise AssertionError('Service error was not reported')
    check('tap_error',error_test)
    check('tap_async_uws_lifecycle',lambda: query(**{'async':True}))
    def cancel_test():
        global hold
        hold=True;event=threading.Event();threading.Timer(.1,event.set).start()
        try:query(**{'async':True,'_cancel':event})
        except InterruptedError:
            assert any('ABORT' in request.get('body','') for request in requests);assert any(request['method']=='DELETE' for request in requests);return {'cancelled':True,'remote_job_aborted_and_deleted':True}
        raise AssertionError('Cancelled job completed')
    check('tap_async_cancel',cancel_test)
    if args.live:
        check('live_vizier_tap_small',lambda:worker.dispatch('vo_query',{'service':'https://tapvizier.cds.unistra.fr/TAPVizieR/tap','protocol':'tap','adql':'SELECT TOP 1 table_name FROM TAP_SCHEMA.tables','max_rows':1}))
finally:
    worker.close();server.shutdown();server.server_close()
report['requests']=requests;report['passed']=all(c['passed'] for c in report['checks']);(args.output/'vo-validation.json').write_text(json.dumps(clean(report),ensure_ascii=False,indent=2),encoding='utf-8');print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'failures':[c['name'] for c in report['checks'] if not c['passed']]}));raise SystemExit(0 if report['passed'] else 1)

