"""Bounded streaming transport for external requests-based astronomy clients."""
import io,math,re,time
from fitsviewer_science import MAX_TABLE

def file_capability_request(worker,params=None):
    """Keep the host's restriction through internal calls. / 内部调用不能放宽文件能力。"""
    return (params or {}).get('_file_capability') is True or getattr(getattr(worker,'scope',None),'file_capability',False) is True


def _table_bytes(body):
    """Decode only bounded table bytes, before a reader can infer another format."""
    if len(body)>MAX_TABLE:raise ValueError('File-capability table exceeds the 64 MiB input limit')
    for layer in range(4):
        if body.startswith(b'\x1f\x8b'):
            import gzip
            opener=lambda stream:gzip.GzipFile(fileobj=stream)
        elif body.startswith(b'BZh'):
            import bz2
            opener=bz2.BZ2File
        elif body.startswith((b'\xfd7zXZ\x00',b'\x5d\x00')):
            import lzma
            opener=lzma.LZMAFile
        elif body.startswith((b'PK\x03\x04',b'PK\x05\x06',b'PK\x07\x08')):
            raise ValueError('File capabilities require a direct self-contained table, not a ZIP archive')
        else:return body
        if layer==3:raise ValueError('File-capability tables have too many compression layers')
        try:
            with opener(io.BytesIO(body)) as stream:body=stream.read(MAX_TABLE+1)
        except (OSError,EOFError) as error:raise ValueError('Invalid compressed file-capability table') from error
        if len(body)>MAX_TABLE:raise ValueError('Decoded file-capability table exceeds the 64 MiB limit')
    raise ValueError('File-capability tables have too many compression layers')


def _xml_table(body):
    head=body.lstrip(b' \t\r\n\xef\xbb\xbf')
    return head.startswith(b'<') or body.startswith((b'\xff\xfe',b'\xfe\xff',b'\x00\x00\xfe\xff')) or head.startswith((b'\x00<',b'<\x00',b'\x00\x00\x00<'))


def require_self_contained_xml(body):
    """Preflight XML without resolving any URI, DTD or custom entity.

    STREAM href can bypass a file grant even when the XML file itself is stable.
    LINK href and DataLink URL fields are ordinary metadata and remain allowed.
    """
    from xml.parsers import expat
    parser=expat.ParserCreate(namespace_separator='}')
    def reject(*args):raise ValueError('File capabilities require self-contained VOTables; DTDs and entity declarations are forbidden')
    def element(name,attributes):
        if name.rsplit('}',1)[-1].upper()=='STREAM' and any(key.rsplit('}',1)[-1].lower()=='href' for key in attributes):
            raise ValueError('File capabilities require self-contained VOTables; external STREAM href is forbidden')
    parser.StartDoctypeDeclHandler=reject;parser.EntityDeclHandler=reject;parser.ExternalEntityRefHandler=reject
    parser.SetParamEntityParsing(expat.XML_PARAM_ENTITY_PARSING_NEVER)
    parser.StartElementHandler=element
    try:parser.Parse(body,True)
    except expat.ExpatError as error:raise ValueError('File capabilities require a well-formed self-contained XML table') from error


def read_table(source,*,file_capability=False,**kwargs):
    """The shared table-reader boundary; trusted sessions keep Astropy's API.

    文件能力先检查同一份有界快照，再把它交给 Astropy；不按临时别名后缀猜格式。
    Only table inputs use this 64 MiB bound; ordinary FITS image reads are unchanged.
    """
    if file_capability:
        from fitsviewer_science import bounded_path
        path=bounded_path(source)
        if path.stat().st_size>MAX_TABLE:raise ValueError('File-capability table exceeds the 64 MiB input limit')
        with path.open('rb') as stream:body=_table_bytes(stream.read(MAX_TABLE+1))
        xml=_xml_table(body);format_=kwargs.get('format')
        if format_ is not None and (not isinstance(format_,str) or format_ not in ['fits','votable','ascii'] and not format_.startswith('ascii.')):
            raise ValueError('File capabilities support self-contained FITS, ASCII/CSV and VOTable tables')
        if xml:require_self_contained_xml(body)
        if format_=='votable' and not xml:raise ValueError('File capabilities require a self-contained XML VOTable')
        if xml:
            kwargs.setdefault('format','votable');source=io.BytesIO(body)
        elif body.startswith((b'SIMPLE  =',b'XTENSION=')) or format_=='fits':
            kwargs.setdefault('format','fits');source=io.BytesIO(body)
        else:
            if format_ is None:
                name=path.name.lower().removesuffix('.gz').removesuffix('.bz2').removesuffix('.xz')
                kwargs['format']='ascii.ecsv' if body.startswith(b'# %ECSV') else 'ascii.csv' if name.endswith('.csv') else 'ascii'
            try:source=body.decode('utf-8-sig').splitlines()
            except UnicodeError as error:raise ValueError('File-capability ASCII/CSV tables must use UTF-8') from error
    from astropy.table import Table
    return Table.read(source,**kwargs)


def bounded_timeout(value):
    """Reject invalid deadlines and retain the 600 s cap. / 拒绝无效时限，保留 600 秒上限。"""
    try:
        if isinstance(value,bool):raise ValueError()
        timeout=float(value)
    except (TypeError,ValueError):raise ValueError('Timeout must be a finite positive number') from None
    if not math.isfinite(timeout) or timeout<=0:raise ValueError('Timeout must be a finite positive number')
    return min(timeout,600.)

def session(cancel=None,timeout=120):
    timeout=bounded_timeout(timeout)
    import requests
    deadline=time.monotonic()+max(timeout,1)
    class Raw:
        def __init__(self,raw,owner):self.raw=raw;self.count=0;self.owner=owner
        def __getattr__(self,name):return getattr(self.raw,name)
        def read(self,amt=None,decode_content=None,**kwargs):
            if self.owner.cancel and self.owner.cancel.is_set():raise InterruptedError('Network operation cancelled')
            if time.monotonic()>self.owner.deadline:raise TimeoutError('Network operation exceeded its deadline')
            amount=min(amt if amt is not None and amt>=0 else MAX_TABLE+1,MAX_TABLE+1-self.count)
            result=self.raw.read(amount,decode_content=decode_content,**kwargs);self.count+=len(result)
            if self.count>MAX_TABLE:raise ValueError('Remote response exceeds 64 MiB')
            return result
        def stream(self,amt=65536,decode_content=None):
            while block:=self.read(amt,decode_content=decode_content):yield block
    class Session(requests.Session):
        def __init__(self):super().__init__();self.cancel=cancel;self.deadline=deadline;self.created_job_url=None
        def request(self,*args,**kwargs):
            kwargs['timeout']=kwargs.get('timeout') or (10,30);kwargs['stream']=True
            response=super().request(*args,**kwargs)
            if str(args[0] if args else kwargs.get('method','')).upper()=='POST' and '/async/' in response.url and response.history:self.created_job_url=response.url
            response.raw=Raw(response.raw,self);return response
    return Session()


class ServiceError(RuntimeError):
    """A usable response was not obtained. / 与合法空表、缺依赖分开报告。"""
    def __init__(self,message,category='service',service=None):
        super().__init__(message);self.category=category;self.service=service


def fetch_bytes(url,*,params=None,data=None,cancel=None,timeout=60,maximum=MAX_TABLE):
    """Bound metadata requests and retry a network timeout once; never retry uploads.

    元数据查询超时只重试一次；HTTP 错误和解析错误不会被伪装为空结果。
    """
    import requests
    timeout=bounded_timeout(timeout);maximum=min(int(maximum),MAX_TABLE)
    if maximum<1:raise ValueError('Response limit must be positive')
    for attempt in range(2):
        if cancel and cancel.is_set():raise InterruptedError('Network operation cancelled')
        try:
            with session(cancel,timeout) as client:
                with client.request('POST' if data is not None else 'GET',url,params=params,data=data,
                                    timeout=(min(10.,timeout),min(30.,timeout)),headers={'User-Agent':'FitsViewer science adapter'}) as response:
                    if response.status_code>=400:raise ServiceError(f'HTTP {response.status_code} from {url}','service',url)
                    length=response.headers.get('Content-Length')
                    if length and int(length)>maximum:raise ServiceError('Remote metadata exceeds the response limit','protocol',url)
                    body=bytearray()
                    for block in response.iter_content(65536):
                        if cancel and cancel.is_set():raise InterruptedError('Network operation cancelled')
                        body.extend(block)
                        if len(body)>maximum:raise ServiceError('Remote metadata exceeds the response limit','protocol',url)
                    return bytes(body),{'url':response.url,'content_type':response.headers.get('Content-Type',''),'encoding':response.encoding,'attempts':attempt+1}
        except (requests.Timeout,TimeoutError) as error:
            if attempt:raise ServiceError(f'Network timeout after one retry: {error}','network',url) from error
        except requests.RequestException as error:
            raise ServiceError(f'Network request failed: {error}','network',url) from error


def parse_votable(body,*,file_capability=False):
    """Preserve FIELD names (IRSA IDs differ), service errors and overflow status."""
    from astropy.io.votable import parse
    if file_capability:body=_table_bytes(body)
    normalized=False
    # SDSS SIAP advertises UTF-16 while its actual bytes are UTF-8. Do not rewrite
    # genuine UTF-16/BOM data or arbitrary malformed XML. / 仅修复已证实的声明矛盾。
    if body.startswith(b'<?xml') and b'\x00' not in body[:160] and re.search(br'encoding=[\"\']utf-16[\"\']',body[:160],re.I):
        body.decode('utf-8')
        body=re.sub(br'encoding=([\"\'])utf-16\1',b'encoding="utf-8"',body,count=1,flags=re.I);normalized=True
    if file_capability:
        require_self_contained_xml(body)
    try:document=parse(io.BytesIO(body),verify='warn')
    except Exception as error:raise ServiceError('Response is not a usable VOTable: '+str(error),'protocol') from error
    status=[]
    def visit(resource):
        for info in resource.infos:
            if str(info.name).upper()=='QUERY_STATUS':status.append((str(info.value).upper(),str(info.content or '')))
        for child in resource.resources:visit(child)
    for resource in document.resources:visit(resource)
    for value,message in status:
        if value=='ERROR':raise ServiceError('VO service query failed: '+message,'service')
    tables=list(document.iter_tables())
    if not tables:raise ServiceError('VOTable contains no result table','protocol')
    selected=tables[0];table=selected.to_table(use_names_over_ids=True)
    mapping={str(field.ID):str(field.name or field.ID) for field in selected.fields if field.ID}
    return table,{'query_status':'OVERFLOW' if any(value=='OVERFLOW' for value,_ in status) else 'OK',
                  'field_id_names':mapping,'xml_declaration_normalized':normalized}

