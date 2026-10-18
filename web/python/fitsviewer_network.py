"""Bounded streaming transport for external requests-based astronomy clients."""
import math,time
from fitsviewer_science import MAX_TABLE

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

