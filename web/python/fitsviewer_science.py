"""On-demand adapter for the user's scientific Python environment.

按需调用用户环境中的科研包；此文件不包含或安装 Astropy/NumPy/Matplotlib。
The line-delimited JSON protocol never evaluates arbitrary Python expressions.
"""
from pathlib import Path
from collections import OrderedDict, deque
from concurrent.futures import ThreadPoolExecutor
import argparse
import contextlib
import importlib.metadata
import importlib.util
import io
import json
import math
import os
import sys
import struct
import subprocess
import tempfile
import threading
import time
import uuid
from fitsviewer_build_profile import require_operation, available_operations

MAX_INPUT=512*1024*1024
MAX_TABLE=64*1024*1024
MAX_JOBS=32
MAX_JOB_RESULT=32*1024*1024
MAX_TERMINAL_BYTES=32*1024*1024
MAX_TERMINALS=32
MAX_CONSUMED_JOBS=128
JOB_IDENTITY_FIELDS=['owner','operation','document_generation','gaia_generation','science_role','role_epoch']

def clean(value):
    if isinstance(value,dict):return {str(k):clean(v) for k,v in value.items()}
    if isinstance(value,(list,tuple,deque)):return [clean(v) for v in value]
    if isinstance(value,float):return value if math.isfinite(value) else None
    if isinstance(value,Path):return str(value)
    if hasattr(value,'tolist'):return clean(value.tolist())
    if isinstance(value,(str,int,bool)) or value is None:return value
    return str(value)

def native_path(value,strict=False,verbatim=False):
    """Keep Windows extended paths for external file APIs, without changing POSIX paths.

    科研缓存强制保留 Windows 长路径前缀，避免拼接产物文件名后超出 MAX_PATH。
    """
    path=Path(value).expanduser()
    if os.name=='nt':
        text=os.path.abspath(path)
        if (verbatim or len(text)>=240) and not text.startswith('\\\\?\\'):
            text='\\\\?\\UNC\\'+text[2:] if text.startswith('\\\\') else '\\\\?\\'+text
        path=Path(text)
    return path.resolve(strict=strict)

def bounded_path(value,limit=MAX_INPUT):
    if not isinstance(value,(str,Path)) or not str(value).strip():raise ValueError('Choose a nonempty input file path')
    path=native_path(value,strict=True)
    if not path.is_file() or path.stat().st_size>limit:raise ValueError('Input exceeds the supported file size')
    return path

def require(*packages):
    missing=[p for p in packages if importlib.util.find_spec(p) is None]
    if missing:raise ModuleNotFoundError('Selected environment is missing '+', '.join(missing)+'. Install in that environment: python -m pip install '+' '.join('scikit-image' if p=='skimage' else p for p in missing))

def preview_values(values):
    def value(item):
        if hasattr(item,'tolist'):item=item.tolist()
        if isinstance(item,(list,tuple)):return [value(v) for v in item[:64]]+(['…'] if len(item)>64 else [])
        if isinstance(item,(str,bytes)):return str(item)[:2048]
        return clean(item)
    return [value(item) for item in values]

class Worker:
    def __init__(self,cache):
        self.cache=native_path(cache,verbatim=True);self.cache.mkdir(parents=True,exist_ok=True)
        self.headers=OrderedDict();self.tables=OrderedDict();self.artifacts={}
        self.table_owners={};self.header_owners={}
        self.cache_lock=threading.RLock()
        self.artifact_owners={};self.artifact_jobs={};self.artifact_documents={};self.artifact_gaia_generations={};self.artifact_roles={};self.artifact_role_epochs={};self.artifact_lock=threading.RLock()
        self.jobs={};self.executor=ThreadPoolExecutor(max_workers=1,thread_name_prefix='fitsviewer-science')
        self.terminals=OrderedDict();self.terminal_bytes=0;self.consumed_jobs=OrderedDict()
        self.events=deque(maxlen=256);self.samp=None;self.samp_owners=set();self.scope=threading.local()

    def cache_owner(self,include_role=True):
        owner=getattr(self.scope,'owner','cli')
        if getattr(self.scope,'document_explicit',False) or getattr(self.scope,'role_epoch',None) is not None:
            return (owner,self.scope.document_generation,self.scope.science_role if include_role else 'view',(self.scope.role_epoch or 0) if include_role else 0)
        return owner

    @staticmethod
    def owner_matches(token,owner,generation=None,role=None,role_epoch=None):
        user,document=token[:2] if isinstance(token,tuple) else (token,0)
        token_role,epoch=token[2:] if isinstance(token,tuple) and len(token)==4 else ('pro',0)
        return user==owner and (generation is None or document<=generation) and (role is None or token_role==role) and (role_epoch is None or epoch<=role_epoch)

    def consumed_job(self,identifier,job,reason):
        """Keep bounded retry metadata, never the future/result. / 仅保留有界重试元数据。"""
        self.consumed_jobs[identifier]={key:job.get(key) for key in JOB_IDENTITY_FIELDS}
        self.consumed_jobs[identifier]['reason']=reason
        self.consumed_jobs.move_to_end(identifier)
        while len(self.consumed_jobs)>MAX_CONSUMED_JOBS:self.consumed_jobs.popitem(last=False)

    def forget_terminal(self,identifier,reason):
        job=self.terminals.pop(identifier)
        self.terminal_bytes-=len(job['payload'])
        self.consumed_job(identifier,job,reason)
        self.discard_artifacts(lambda key:self.artifact_jobs.get(key)==identifier)

    def reap_jobs(self):
        """Cancel aged active jobs, then bound completed history. / 活任务先取消，终态有界。"""
        now=time.time()
        for identifier,job in list(self.jobs.items()):
            future=job['future']
            if now-job['created']>=900 and not future.done():job['cancel'].set();future.cancel()
            if not future.done():continue
            try:
                if future.cancelled() or job['cancel'].is_set():
                    response={'state':'cancelled'}
                    self.discard_artifacts(lambda key:self.artifact_jobs.get(key)==identifier)
                else:response={'state':'completed','result':future.result()}
            except InterruptedError:response={'state':'cancelled'}
            except Exception as error:response={'state':'failed','error':str(error),'error_type':type(error).__name__,'error_category':getattr(error,'category','missing_dependency' if isinstance(error,ModuleNotFoundError) else 'operation')}
            payload=bytearray()
            for chunk in json.JSONEncoder(ensure_ascii=False,allow_nan=False,separators=(',',':')).iterencode(clean(response)):
                encoded=chunk.encode('utf-8')
                if len(payload)+len(encoded)>MAX_JOB_RESULT:payload=None;break
                payload.extend(encoded)
            self.jobs.pop(identifier,None)
            if payload is None:
                self.consumed_job(identifier,job,'result_too_large')
                self.discard_artifacts(lambda key:self.artifact_jobs.get(key)==identifier)
                continue
            terminal={key:job.get(key) for key in JOB_IDENTITY_FIELDS}
            terminal.update(payload=bytes(payload),created=now)
            self.terminals[identifier]=terminal;self.terminal_bytes+=len(payload)
        for identifier,job in list(self.terminals.items()):
            if now-job['created']>=900:self.forget_terminal(identifier,'expired')
        while len(self.terminals)>MAX_TERMINALS or self.terminal_bytes>MAX_TERMINAL_BYTES:
            self.forget_terminal(next(iter(self.terminals)),'history_limit')

    def owned_job(self,identifier):
        job=self.jobs.get(identifier) or self.terminals.get(identifier) or self.consumed_jobs.get(identifier)
        if job is None:raise ValueError('Job is unavailable')
        if job['owner']!=self.scope.owner:raise ValueError('Job belongs to a different viewer session')
        return job

    def capabilities(self,params):
        packages={}
        for name in ['numpy','astropy','specutils','matplotlib','scipy','photutils','pyvo','astroquery','astroalign','reproject','scikit-image','shapely','requests']:
            try:packages[name]=importlib.metadata.version(name)
            except importlib.metadata.PackageNotFoundError:packages[name]=None
        return {'protocol':1,'python':sys.executable,'prefix':sys.prefix,'packages':packages,
                'loaded_modules':[p for p in ['numpy','astropy','specutils','matplotlib','scipy','photutils','pyvo','astroquery','reproject','skimage'] if p in sys.modules],
                'bundled_scientific_packages':False,'embedded_tex':False,
                'operations':available_operations(['wcs','catalogue','xmatch','identify_stars','detect_stars','detect_lines','trace_curve','reproject','rgb','export','vo_query','vo_download','vo_datalink','archive_search','service_presets','dependency_probe','dependency_install','samp','region_transform','line_profile','gaia_solve','spectral_candidates','spectral_analysis','image_flux_match','math_preview','xmatch_cds','sky_point','view_link','job_start','job_status','job_cancel'])}

    def image(self,params,data=False):
        require('numpy','astropy')
        from astropy.io import fits
        path=bounded_path(params['path']);selected=params.get('hdu')
        with fits.open(path,memmap=False,lazy_load_hdus=True) as hdus:
            if selected is None:selected=next((i for i,h in enumerate(hdus) if len(getattr(h,'shape',()) or ())==2 and not isinstance(h,fits.BinTableHDU)),None)
            if selected is None or not 0<=int(selected)<len(hdus):raise ValueError('No supported two-dimensional image HDU')
            hdu=hdus[int(selected)]
            if len(hdu.shape or ())!=2:raise ValueError('This tool needs a two-dimensional image')
            if math.prod(hdu.shape)>32*1024*1024:raise ValueError('Image exceeds 32 Mi pixels')
            header=hdu.header.copy();array=hdu.data.astype('float64') if data and not os.environ.get('FITSVIEWER_CORE_BINARY') else None
        if data and os.environ.get('FITSVIEWER_CORE_BINARY'):
            dataset=self.dataset(dict(params,hdu=int(selected)));array=dataset['values']
            if array.ndim!=2:raise ValueError('This tool needs a two-dimensional image')
        from fitsviewer_wcs import overlay
        view=params.get('view') if isinstance(params.get('view'),dict) else {}
        header=overlay(header,params.get('wcs_override',view.get('wcs_override')))
        return path,int(selected),header,array

    def dataset(self,params):
        """Use the same fitsrs packet as GUI/HTTP when the core executable is available."""
        require('numpy')
        import numpy as np
        binary=os.environ.get('FITSVIEWER_CORE_BINARY')
        if not binary:raise RuntimeError('The shared FITS core executable was not configured')
        command=[binary,'--packet',str(bounded_path(params['path']))]
        for key,flag in [('hdu','--hdu'),('x_column','--x-column'),('y_column','--y-column'),('error_column','--error-column'),('error_hdu','--error-hdu')]:
            value=params.get(key)
            if value is not None:
                limit=4095 if key in ('hdu','error_hdu') else 255
                if isinstance(value,bool) or not isinstance(value,int) or not 0<=value<=limit:raise ValueError(f'{key} must be an integer in [0, {limit}]')
                command.extend([flag,str(value)])
        if 'no_error' in params and not isinstance(params['no_error'],bool):raise ValueError('no_error must be boolean')
        if params.get('no_error'):command.append('--no-error')
        if params.get('error_kind') is not None:
            if params['error_kind'] not in ('auto','sigma','variance','ivar'):raise ValueError('error_kind must be auto, sigma, variance or ivar')
            command.extend(['--error-kind',params['error_kind']])
        options={'creationflags':subprocess.CREATE_NO_WINDOW} if os.name=='nt' else {}
        self.check_cancelled()
        # Disk-backed capture avoids pipe deadlocks and can be interrupted while the core runs.
        # 捕获使用自动关闭的临时文件，取消时回收本次子进程，不持有无界管道缓冲。
        with tempfile.TemporaryFile(dir=self.cache) as output,tempfile.TemporaryFile(dir=self.cache) as errors:
            process=subprocess.Popen(command,stdout=output,stderr=errors,stdin=subprocess.DEVNULL,env=dict(os.environ,FITSVIEWER_OWNED_PACKET='1',FITSVIEWER_PACKET_PARENT=str(os.getpid())),**options)
            deadline=time.monotonic()+60
            try:
                while process.poll() is None:
                    self.check_cancelled()
                    if time.monotonic()>=deadline:raise TimeoutError('FITS core dataset extraction exceeded 60 seconds')
                    if os.fstat(output.fileno()).st_size>MAX_INPUT or os.fstat(errors.fileno()).st_size>1024*1024:raise ValueError('FITS core output exceeds the bounded capture limit')
                    cancel=getattr(self.scope,'cancel',None)
                    if cancel:cancel.wait(.025)
                    else:time.sleep(.025)
                self.check_cancelled()
                if os.fstat(output.fileno()).st_size>MAX_INPUT or os.fstat(errors.fileno()).st_size>1024*1024:raise ValueError('FITS core output exceeds the bounded capture limit')
                if process.returncode:
                    errors.seek(0);raise ValueError(errors.read(1024*1024).decode('utf-8','replace').strip() or 'FITS core rejected the dataset')
                output.seek(0);packet=output.read(MAX_INPUT+1)
            finally:
                if process.poll() is None:process.kill()
                process.wait()
        if len(packet)<4:raise ValueError('Incomplete core dataset')
        length=struct.unpack_from('<I',packet)[0]
        if length>8*1024*1024 or len(packet)<4+length:raise ValueError('Invalid core packet metadata length')
        meta=json.loads(packet[4:4+length]);count=meta['width']*meta['height'];bp=meta['bitpix'];dtype={8:'u1',16:'<i2',32:'<i4',64:'<i8',-32:'<f4',-64:'<f8'}[bp]
        if not 0<count<=32*1024*1024:raise ValueError('Core packet dimensions exceed the sample limit')
        size=abs(bp)//8;raw=np.frombuffer(packet,dtype=dtype,count=count,offset=4+length);values=raw.astype('float64');valid=np.isfinite(values)
        if bp>0 and meta.get('blank') is not None:valid&=raw!=meta['blank']
        values*=meta['bscale'];values+=meta['bzero'];values[~valid]=np.nan
        result={'meta':meta,'values':values.reshape(meta['height'],meta['width'])}
        if meta.get('spectrum'):
            result['values']=values;result['x']=np.frombuffer(packet,dtype='<f8',count=count,offset=4+length+count*size).copy()
            if meta['spectrum'].get('error_column') is not None or meta['spectrum'].get('error_hdu') is not None:result['error']=np.frombuffer(packet,dtype='<f8',count=count,offset=4+length+count*size+count*8).copy()
        return result

    def celestial(self,params):
        from astropy.wcs import WCS
        from fitsviewer_wcs import model_key
        path=bounded_path(params['path']);key=model_key(params,path)
        with self.cache_lock:
            self.check_cancelled()
            cached=self.headers.get(key)
        if cached is None:
            _,_,header,_=self.image(params)
            wcs=WCS(header,naxis=2).celestial
            if not wcs.has_celestial:raise ValueError('No usable celestial WCS; pixel coordinates remain available')
            if model_key(params,path)!=key:raise ValueError('FITS file changed while preparing WCS; reopen the image')
            cached=(header,wcs)
        with self.cache_lock:
            self.check_cancelled()
            self.headers.setdefault(key,cached)
            while len(self.headers)>12:
                old,_=self.headers.popitem(last=False);self.header_owners.pop(old,None)
            self.header_owners.setdefault(key,set()).add(self.cache_owner(include_role=False))
            self.headers.move_to_end(key)
            return self.headers[key]

    def wcs(self,params):
        require('numpy','astropy')
        import numpy as np
        from astropy.coordinates import FK5
        from astropy.time import Time
        from astropy.wcs.utils import pixel_to_skycoord,skycoord_to_pixel,wcs_to_celestial_frame
        import astropy.units as u
        header,wcs=self.celestial(params)
        points=np.asarray(params.get('points',[[float(header['NAXIS1'])/2-.5,float(header['NAXIS2'])/2-.5]]),dtype=float)
        if points.ndim!=2 or points.shape[1]!=2 or len(points)>512 or not np.isfinite(points).all():raise ValueError('Expected at most 512 finite zero-based pixel pairs')
        sky=pixel_to_skycoord(points[:,0],points[:,1],wcs,origin=0,mode='all')
        j2000=sky.transform_to(FK5(equinox=Time('J2000')))
        result={'frame':'FK5','equinox':'J2000','source_frame':str(wcs_to_celestial_frame(wcs)),
                'points':[{'pixel':p,'ra_deg':ra,'dec_deg':dec,'sexagesimal':coord.to_string('hmsdms',precision=3)} for p,ra,dec,coord in zip(points,j2000.ra.deg,j2000.dec.deg,j2000)],
                'distortion':bool(wcs.has_distortion),'coordinate_origin':0}
        if len(points):
            x,y=points[0];center=sky[0]
            north=center.directional_offset_by(0*u.deg,1*u.arcsec);east=center.directional_offset_by(90*u.deg,1*u.arcsec)
            nx,ny=skycoord_to_pixel(north,wcs,origin=0,mode='all');ex,ey=skycoord_to_pixel(east,wcs,origin=0,mode='all')
            nearby=pixel_to_skycoord([x+.5,x-.5,x,x],[y,y,y+.5,y-.5],wcs,origin=0,mode='all')
            result['compass']={'pixel_anchor':[x,y],'north':[nx-x,ny-y],'east':[ex-x,ey-y],
                               'arcsec_per_pixel_x':nearby[0].separation(nearby[1]).arcsec,'arcsec_per_pixel_y':nearby[2].separation(nearby[3]).arcsec,'local_only':True}
        return clean(result)

    def image_center(self,params):
        """Use the image geometry, never the most recent cursor WCS request.

        图像中心与局部角尺度来自当前HDU/override，供重投影与在线天区查询共用。
        """
        require('numpy','astropy')
        import numpy as np
        from astropy.coordinates import FK5
        from astropy.time import Time
        from astropy.wcs.utils import pixel_to_skycoord
        header,wcs=self.celestial(params);width,height=int(header['NAXIS1']),int(header['NAXIS2'])
        x,y=(width-1)/2,(height-1)/2
        center=pixel_to_skycoord(x,y,wcs,origin=0,mode='all');icrs=center.icrs;j2000=center.transform_to(FK5(equinox=Time('J2000')))
        near=pixel_to_skycoord([x-.5,x+.5,x,x],[y,y,y-.5,y+.5],wcs,origin=0,mode='all')
        sx,sy=float(near[0].separation(near[1]).arcsec),float(near[2].separation(near[3]).arcsec)
        if not np.isfinite([sx,sy,icrs.ra.deg,icrs.dec.deg]).all() or min(sx,sy)<=0:raise ValueError('Image center has no finite celestial position/pixel scale')
        return {'pixel':[x,y],'icrs':{'ra_deg':float(icrs.ra.deg),'dec_deg':float(icrs.dec.deg)},
                'j2000':{'ra_deg':float(j2000.ra.deg),'dec_deg':float(j2000.dec.deg)},
                'scale_arcsec':math.sqrt(sx*sy),'scale_x_arcsec':sx,'scale_y_arcsec':sy,'local_only':True}

    def catalogue(self,params):
        require('numpy','astropy')
        import numpy as np
        from fitsviewer_network import read_table,file_capability_request
        from astropy.coordinates import SkyCoord,FK5
        from astropy.time import Time
        import astropy.units as u
        path=bounded_path(params['catalogue'],MAX_TABLE)
        kwargs={}
        if params.get('format'):kwargs['format']=params['format']
        if params.get('table_hdu') is not None:kwargs['hdu']=int(params['table_hdu'])
        if path.suffix.lower() in ['.vot','.votable','.xml']:kwargs.setdefault('format','votable')
        file_capability=file_capability_request(self,params)
        key=(str(path),path.stat().st_mtime_ns,json.dumps(kwargs,sort_keys=True),file_capability)
        with self.cache_lock:table=self.tables.get(key)
        if table is None:table=read_table(path,file_capability=file_capability,**kwargs)
        with self.cache_lock:
            self.check_cancelled()
            table=self.tables.setdefault(key,table)
            while len(self.tables)>3:
                old,_=self.tables.popitem(last=False);self.table_owners.pop(old,None)
            self.table_owners.setdefault(key,set()).add(self.cache_owner())
            self.tables.move_to_end(key)
        count=len(table)
        if count>200000:raise ValueError('Catalogue exceeds 200,000 rows; select a smaller table')
        names=table.colnames
        start=max(0,min(int(params.get('preview_start',0)),max(0,count-1)))
        preview={name:preview_values(table[name][start:start+100]) for name in names[:64]}
        result={'columns':[{'name':name,'unit':str(table[name].unit or ''),'dtype':str(table[name].dtype)} for name in names[:256]],'rows':count,'preview':preview,'preview_start':start,'preview_limit':100,'markers':[]}
        if not params.get('ra_column') or not params.get('dec_column'):return result
        missing=[name for name in [params['ra_column'],params['dec_column'],params.get('id_column')] if name and name not in names]
        if missing:raise ValueError('Selected catalogue columns are absent: '+', '.join(missing))
        ra=table[params['ra_column']];dec=table[params['dec_column']]
        unit=(u.Unit(params.get('ra_unit') or ra.unit or 'deg'),u.Unit(params.get('dec_unit') or dec.unit or 'deg'))
        kwargs={'frame':params.get('frame','icrs').lower()}
        if kwargs['frame'] in ['fk5','fk4']:kwargs['equinox']=Time(params.get('equinox','J2000' if kwargs['frame']=='fk5' else 'B1950'))
        if params.get('pm_ra_column') and params.get('pm_dec_column'):
            pm_ra=table[params['pm_ra_column']];pm_dec=table[params['pm_dec_column']]
            kwargs.update(pm_ra_cosdec=np.asarray(pm_ra)*(pm_ra.unit or u.mas/u.yr),pm_dec=np.asarray(pm_dec)*(pm_dec.unit or u.mas/u.yr),obstime=Time(params['catalogue_epoch']))
        sky=SkyCoord(np.ma.asarray(ra,dtype=float).filled(np.nan),np.ma.asarray(dec,dtype=float).filled(np.nan),unit=unit,**kwargs)
        if params.get('target_epoch') and 'obstime' in kwargs:sky=sky.apply_space_motion(new_obstime=Time(params['target_epoch']))
        j2000=sky.transform_to(FK5(equinox=Time('J2000')))
        if params.get('include_coordinates',True):result['coordinates']={'frame':'FK5','equinox':'J2000','ra_deg':clean(j2000.ra.deg),'dec_deg':clean(j2000.dec.deg)}
        result['coordinate_model']={key:params.get(key) for key in ['frame','equinox','ra_unit','dec_unit','catalogue_epoch','target_epoch','pm_ra_column','pm_dec_column']}
        if params.get('path'):
            from astropy.wcs.utils import skycoord_to_pixel
            header,wcs=self.celestial(params);x,y=skycoord_to_pixel(sky,wcs,origin=0,mode='all')
            visible=np.flatnonzero(np.isfinite(x)&np.isfinite(y)&(x>=-.5)&(x<header['NAXIS1']-.5)&(y>=-.5)&(y<header['NAXIS2']-.5))
            limit=max(1,min(int(params.get('marker_limit',5000)),20000));result['visible_rows']=len(visible);result['marker_limit']=limit
            result['markers']=[{'row':int(i),'x':float(x[i]),'y':float(y[i]),'ra_deg':float(j2000.ra.deg[i]),'dec_deg':float(j2000.dec.deg[i]),'id':str(table[params['id_column']][i]) if params.get('id_column') else str(i)} for i in visible[:limit]]
        return result

    def view_link(self,params):
        require('numpy','astropy')
        import numpy as np
        from astropy.wcs.utils import pixel_to_skycoord,skycoord_to_pixel
        _,source=self.celestial({'path':params['source_path'],'hdu':params.get('source_hdu'),'wcs_override':params.get('source_wcs_override')});_,target=self.celestial(params)
        center=np.asarray(params['center'],dtype=float)
        if center.shape!=(2,) or not np.isfinite(center).all():raise ValueError('Expected a finite source view center')
        points=np.array([center,center+[1.,0.],center+[0.,1.]])
        sky=pixel_to_skycoord(points[:,0],points[:,1],source,origin=0,mode='all');x,y=skycoord_to_pixel(sky,target,origin=0,mode='all');mapped=np.column_stack((x,y));ratio=float(np.sqrt(abs(np.linalg.det(np.array([mapped[1]-mapped[0],mapped[2]-mapped[0]])))))
        if not np.isfinite(mapped).all() or not np.isfinite(ratio) or ratio<=0:raise ValueError('The linked position is outside a valid projection')
        return {'center':mapped[0].tolist(),'ratio':ratio,'link':'sky center and local angular scale; display orientation is retained'}

    def sky_point(self,params):
        require('astropy','numpy')
        from astropy.coordinates import SkyCoord,FK5
        from astropy.time import Time
        from astropy.wcs.utils import skycoord_to_pixel
        import numpy as np
        points=np.asarray(params.get('points',[[params.get('ra_deg'),params.get('dec_deg')]]),dtype=float)
        if points.ndim!=2 or points.shape[1]!=2 or not 1<=len(points)<=256 or not np.isfinite(points).all() or np.any(np.abs(points[:,1])>90):raise ValueError('Expected one to 256 finite J2000 coordinate pairs')
        _,wcs=self.celestial(params);sky=SkyCoord(points[:,0],points[:,1],unit='deg',frame=FK5(equinox=Time('J2000')))
        x,y=skycoord_to_pixel(sky,wcs,origin=0,mode='all')
        if not np.isfinite([x,y]).all():raise ValueError('Coordinate is outside the projection')
        pixels=np.column_stack((x,y)).tolist()
        return {'pixel':pixels[0],'pixels':pixels,'indices':params.get('indices',[]),'request_id':params.get('request_id'),'frame':'FK5','equinox':'J2000'}

    def xmatch(self,params):
        require('numpy','astropy','scipy')
        import numpy as np
        from astropy.coordinates import SkyCoord,search_around_sky
        import astropy.units as u
        first=params['first'];second=params['second'];radius=float(params.get('radius_arcsec',1.))
        if isinstance(first,dict) and first.get('catalogue'):first=self.catalogue(first)['coordinates']
        if isinstance(second,dict) and second.get('catalogue'):second=self.catalogue(second)['coordinates']
        if not 0<radius<=3600:raise ValueError('Match radius must be between zero and 3600 arcsec')
        arrays=[np.asarray(item[key],dtype=float) for item in (first,second) for key in ('ra_deg','dec_deg')]
        if any(a.ndim!=1 or len(a)>200000 for a in arrays) or len(arrays[0])!=len(arrays[1]) or len(arrays[2])!=len(arrays[3]):raise ValueError('Expected equal length coordinate columns, up to 200,000 rows')
        valid=[np.flatnonzero(np.isfinite(arrays[i])&np.isfinite(arrays[i+1])&(np.abs(arrays[i+1])<=90)) for i in (0,2)]
        a=SkyCoord(arrays[0][valid[0]],arrays[1][valid[0]],unit='deg',frame=first.get('frame','icrs').lower());b=SkyCoord(arrays[2][valid[1]],arrays[3][valid[1]],unit='deg',frame=second.get('frame','icrs').lower())
        if len(a)==0 or len(b)==0:return {'matches':[],'unmatched_first':valid[0].tolist(),'invalid_first':np.flatnonzero(~np.isin(np.arange(len(arrays[0])),valid[0])).tolist(),'geometric_only':True}
        ia,ib,separation,_=search_around_sky(a,b,radius*u.arcsec)
        if len(ia)>200000:raise ValueError('Match output exceeds 200,000 pairs; reduce radius/input')
        counts=np.bincount(ia,minlength=len(a));counts_b=np.bincount(ib,minlength=len(b));pairs=[{'first':int(valid[0][x]),'second':int(valid[1][y]),'distance_arcsec':float(d),'ambiguous':bool(counts[x]>1 or counts_b[y]>1)} for x,y,d in zip(ia,ib,separation.arcsec)]
        pairs.sort(key=lambda p:(p['first'],p['distance_arcsec'],p['second']))
        if params.get('strategy','nearest')=='nearest':
            seen=set();pairs=[p for p in pairs if p['first'] not in seen and not seen.add(p['first'])]
        elif params['strategy'] not in ['all','unmatched']:raise ValueError('Strategy must be nearest, all or unmatched')
        unmatched=valid[0][np.flatnonzero(counts==0)].tolist()
        if params.get('strategy')=='unmatched':pairs=[]
        return {'matches':pairs,'unmatched_first':unmatched,'invalid_first':np.flatnonzero(~np.isin(np.arange(len(arrays[0])),valid[0])).tolist(),'invalid_second':np.flatnonzero(~np.isin(np.arange(len(arrays[2])),valid[1])).tolist(),'geometric_only':True,'one_to_one_global_assignment':False,'radius_arcsec':radius}

    def artifact(self,path,kind,**metadata):
        path=native_path(path)
        if not path.is_relative_to(self.cache):raise ValueError('Artifact must stay inside the owned cache')
        with self.artifact_lock:
            cancel=getattr(self.scope,'cancel',None)
            if cancel and cancel.is_set():
                path.unlink(missing_ok=True);raise InterruptedError('Scientific operation cancelled')
            if len(self.artifacts)>=64 or sum(p.stat().st_size for p in self.artifacts.values() if p.is_file())+path.stat().st_size>512*1024*1024:
                path.unlink(missing_ok=True);raise ValueError('Session cache limit reached; close unused views')
            identifier=uuid.uuid4().hex;self.artifacts[identifier]=path
            group=getattr(self.scope,'science_role',None) or ('gaia' if getattr(self.scope,'operation','')=='gaia_solve' else 'pro')
            self.artifact_owners[identifier]=(getattr(self.scope,'owner','cli'),group)
            self.artifact_jobs[identifier]=getattr(self.scope,'job',None)
            self.artifact_documents[identifier]=getattr(self.scope,'document_generation',0)
            self.artifact_gaia_generations[identifier]=getattr(self.scope,'gaia_generation',None)
            self.artifact_roles[identifier]=getattr(self.scope,'science_role',group)
            self.artifact_role_epochs[identifier]=getattr(self.scope,'role_epoch',None) or 0
            return {'artifact_id':identifier,'_path':str(path),'name':path.name,'kind':kind,'bytes':path.stat().st_size,'artifact_group':group,'document_generation':self.artifact_documents[identifier],'science_role':self.artifact_roles[identifier],'role_epoch':self.artifact_role_epochs[identifier],**metadata}

    def detach_artifacts(self,identifiers):
        """Host accepted ownership; forget bookkeeping without unlink. / 宿主接管后只移交。"""
        if not isinstance(identifiers,list) or len(identifiers)>64 or any(not isinstance(key,str) for key in identifiers):raise ValueError('Expected at most 64 artifact IDs')
        with self.artifact_lock:
            detached=[]
            for key in identifiers:
                if self.artifacts.pop(key,None) is not None:detached.append(key)
                for table in [self.artifact_owners,self.artifact_jobs,self.artifact_documents,self.artifact_gaia_generations,self.artifact_roles,self.artifact_role_epochs]:table.pop(key,None)
            return {'detached_ids':detached}

    def discard_artifacts(self,predicate):
        with self.artifact_lock:
            for identifier in list(self.artifacts):
                if predicate(identifier):
                    self.artifacts.pop(identifier).unlink(missing_ok=True)
                    for table in [self.artifact_owners,self.artifact_jobs,self.artifact_documents,self.artifact_gaia_generations,self.artifact_roles,self.artifact_role_epochs]:table.pop(identifier,None)

    def check_cancelled(self):
        cancel=getattr(self.scope,'cancel',None)
        if cancel and cancel.is_set():raise InterruptedError('Scientific operation cancelled')

    def release_cached_owner(self,owner,headers=False,generation=None,role=None,role_epoch=None):
        with self.cache_lock:
            for cache,owners in [(self.tables,self.table_owners),*([(self.headers,self.header_owners)] if headers else [])]:
                for key,users in list(owners.items()):
                    users.difference_update([token for token in users if self.owner_matches(token,owner,generation,role,role_epoch)])
                    if not users:cache.pop(key,None);owners.pop(key,None)

    def reproject(self,params,cancel=None):
        require('numpy','astropy')
        import numpy as np
        from astropy.wcs import WCS
        from astropy.io import fits
        from astropy.wcs.utils import pixel_to_skycoord,skycoord_to_pixel
        _,hdu,header,data=self.image(params,data=True);source=WCS(header,naxis=2).celestial
        if not source.has_celestial:raise ValueError('Reprojection requires celestial WCS')
        width=int(params.get('width',512));height=int(params.get('height',512))
        if min(width,height)<2 or max(width,height)>4096 or width*height>16*1024*1024:raise ValueError('Target must be between 2 and 4096 pixels on each side')
        projection=params.get('projection','TAN')
        if projection not in ['TAN','SIN','ARC','STG','ZEA','CAR','AIT','MOL']:raise ValueError('Select a supported standard projection')
        target=WCS(naxis=2);target.wcs.ctype=['RA---'+projection,'DEC--'+projection];target.wcs.radesys='FK5';target.wcs.equinox=2000.
        use_center=bool(params.get('use_image_center'));center=self.image_center(params) if use_center or params.get('scale_arcsec',1.) is None else None
        target.wcs.crval=[center['j2000']['ra_deg'],center['j2000']['dec_deg']] if use_center else [float(params['ra_deg']),float(params['dec_deg'])]
        target.wcs.crpix=[(width+1)/2,(height+1)/2]
        requested_scale=params.get('scale_arcsec');requested_scale=requested_scale if requested_scale is not None else center['scale_arcsec'] if center else 1.
        scale=float(requested_scale)/3600.;angle=math.radians(float(params.get('rotation_deg',0.)))
        if not 0<scale<=10:raise ValueError('Invalid target pixel scale')
        target.wcs.cd=np.array([[-scale*math.cos(angle),scale*math.sin(angle)],[scale*math.sin(angle),scale*math.cos(angle)]])
        output=np.full((height,width),np.nan,dtype='float32')
        for row in range(0,height,64):
            if cancel and cancel.is_set():raise InterruptedError('Reprojection cancelled')
            y,x=np.mgrid[row:min(row+64,height),:width];sky=pixel_to_skycoord(x,y,target,origin=0,mode='all');sx,sy=skycoord_to_pixel(sky,source,origin=0,mode='all')
            valid=np.isfinite(sx)&np.isfinite(sy)&(sx>=0)&(sy>=0)&(sx<data.shape[1]-1)&(sy<data.shape[0]-1)
            ix=np.where(valid,sx,0).astype(int);iy=np.where(valid,sy,0).astype(int);dx=sx-ix;dy=sy-iy
            values=data[iy,ix]*(1-dx)*(1-dy)+data[iy,ix+1]*dx*(1-dy)+data[iy+1,ix]*(1-dx)*dy+data[iy+1,ix+1]*dx*dy
            output[row:row+len(y)]=np.where(valid,values,np.nan)
        result_header=target.to_header();result_header['BUNIT']=header.get('BUNIT','');result_header['HISTORY']='FitsViewer visualization reprojection: bilinear, not flux conserving'
        path=self.cache/('projection-'+uuid.uuid4().hex+'.fits');fits.PrimaryHDU(output,header=result_header).writeto(path)
        return self.artifact(path,'fits',visualization_only=True,flux_conserving=False,source_hdu=hdu,projection=projection,
                             center_j2000={'ra_deg':float(target.wcs.crval[0]),'dec_deg':float(target.wcs.crval[1])},scale_arcsec=scale*3600.,use_image_center=use_center)

    def dispatch(self,operation,params):
        # Internal host bookkeeping needs no PRO adapter. / 宿主内部移交与文档释放不依赖 PRO。
        if operation not in ['artifact_detach','release_document']:require_operation(operation,params)
        self.scope.owner=params.get('_viewer_session','cli')
        self.scope.file_capability=params.get('_file_capability') is True
        if not isinstance(self.scope.owner,str) or len(self.scope.owner)>128:raise ValueError('Invalid viewer session')
        generation=params.get('_document_generation',0)
        if isinstance(generation,bool) or not isinstance(generation,int) or not 0<=generation<2**64:raise ValueError('Invalid document generation')
        self.scope.document_generation=generation;self.scope.document_explicit='_document_generation' in params
        gaia_generation=params.get('_gaia_generation')
        if gaia_generation is not None and (isinstance(gaia_generation,bool) or not isinstance(gaia_generation,int) or not 0<=gaia_generation<2**64):raise ValueError('Invalid Gaia generation')
        self.scope.gaia_generation=gaia_generation
        role=params.get('_science_role','gaia' if operation=='gaia_solve' or (operation=='job_start' and params.get('operation')=='gaia_solve') else 'pro')
        epoch=params.get('_role_epoch')
        if role not in ['pro','gaia','view'] or (epoch is not None and (isinstance(epoch,bool) or not isinstance(epoch,int) or not 0<=epoch<2**64)):raise ValueError('Invalid science role or epoch')
        self.scope.science_role=role;self.scope.role_epoch=epoch
        self.scope.operation=operation
        if operation in ['release_owner','release_pro_owner','release_gaia_owner','release_document']:
            self.reap_jobs()
            cutoff=generation if operation=='release_document' and '_document_generation' in params else None
            release_role='pro' if operation=='release_pro_owner' else 'gaia' if operation=='release_gaia_owner' else None
            release_epoch=epoch if release_role is not None else None
            def selected(job):
                job_role=job.get('science_role') or ('gaia' if job.get('operation')=='gaia_solve' else 'pro')
                if job.get('owner')!=self.scope.owner:return False
                if cutoff is not None and job.get('document_generation',0)>cutoff:return False
                if release_role is not None and job_role!=release_role:return False
                if release_epoch is not None and (job.get('role_epoch') or 0)>release_epoch:return False
                return operation!='release_gaia_owner' or gaia_generation is None or gaia_generation==job.get('gaia_generation')
            for identifier,job in list(self.jobs.items()):
                if selected(job):job['cancel'].set();job['future'].cancel()
            for identifier,job in list(self.terminals.items()):
                if selected(job):self.forget_terminal(identifier,'owner_released')
            owner=self.scope.owner
            def artifact_selected(identifier):
                user,group=self.artifact_owners.get(identifier,(None,None))
                return selected({'owner':user,'operation':'gaia_solve' if group=='gaia' else 'export','document_generation':self.artifact_documents.get(identifier,0),'gaia_generation':self.artifact_gaia_generations.get(identifier),'science_role':self.artifact_roles.get(identifier,group),'role_epoch':self.artifact_role_epochs.get(identifier,0)})
            self.discard_artifacts(artifact_selected)
            if operation!='release_gaia_owner':self.release_cached_owner(owner,headers=operation in ['release_owner','release_document'],generation=cutoff,role=release_role,role_epoch=release_epoch)
            response={'released':True}
            if operation!='release_gaia_owner' and (self.samp or getattr(self,'samp_http',None)):
                from fitsviewer_samp import release_owner
                try:release_owner(self,owner,cutoff,release_epoch)
                except Exception as error:response['samp_cleanup_error']=str(error)
            self.reap_jobs()
            return response
        if operation=='artifact_detach':return self.detach_artifacts(params.get('artifact_ids'))
        if operation=='environment_busy':
            jobs=[{'id':identifier,'state':'running' if job['future'].running() else 'queued'} for identifier,job in self.jobs.items() if not job['future'].done()]
            pending=sum(job['future'].done() and not job['future'].cancelled() and not (job.get('cancel') and job['cancel'].is_set()) for job in self.jobs.values())+sum(not job.get('delivered',False) for job in self.terminals.values())
            return {'active_jobs':len(jobs),'jobs':jobs,'pending_results':pending,'installing':any(job['operation']=='dependency_install' and not job['future'].done() for job in self.jobs.values())}
        if operation=='artifact_path':
            path=self.artifacts.get(params['artifact_id'])
            if path is None or not path.is_file():raise ValueError('Session artifact is unavailable')
            if self.artifact_owners[params['artifact_id']][0]!=self.scope.owner:raise ValueError('Artifact belongs to a different viewer session')
            return {'path':str(path)}
        if operation=='job_start':
            self.reap_jobs()
            target=params['operation']
            if target not in ['reproject','export','vo_query','vo_download','vo_datalink','archive_search','identify_stars','dependency_probe','dependency_install','rgb','detect_stars','detect_lines','trace_curve','xmatch_cds','gaia_solve','spectral_candidates','spectral_analysis','image_flux_match']:raise ValueError('This operation is not a background job')
            if sum(not job['future'].done() for job in self.jobs.values())>=MAX_JOBS:raise ValueError('Too many queued jobs')
            options=dict(params.get('params',{}));document=options.get('_document_generation',generation)
            if isinstance(document,bool) or not isinstance(document,int) or not 0<=document<2**64:raise ValueError('Invalid document generation')
            job_role=options.get('_science_role',role);job_epoch=options.get('_role_epoch',epoch)
            if target in ['dependency_probe','dependency_install']:job_role='view'
            if job_role not in ['pro','gaia','view'] or (job_role=='view' and target not in ['dependency_probe','dependency_install']) or (job_epoch is not None and (isinstance(job_epoch,bool) or not isinstance(job_epoch,int) or not 0<=job_epoch<2**64)):raise ValueError('Invalid science role or epoch')
            job_gaia=options.get('_gaia_generation',gaia_generation)
            if job_gaia is not None and (isinstance(job_gaia,bool) or not isinstance(job_gaia,int) or not 0<=job_gaia<2**64):raise ValueError('Invalid Gaia generation')
            identifier=uuid.uuid4().hex;cancel=threading.Event();job={'cancel':cancel,'created':time.time(),'operation':target,'owner':self.scope.owner,'document_generation':document,'gaia_generation':job_gaia,'science_role':job_role,'role_epoch':job_epoch}
            options.update(_viewer_session=self.scope.owner,_job_id=identifier,_document_generation=document,_file_capability=self.scope.file_capability)
            options.update(_science_role=job_role,_role_epoch=job_epoch,_gaia_generation=job_gaia)
            job['future']=self.executor.submit(self.run_job,target,options,cancel);self.jobs[identifier]=job
            return {'job_id':identifier,'state':'queued'}
        if operation=='job_status':
            self.reap_jobs();identifier=params['job_id'];job=self.owned_job(identifier)
            if identifier in self.terminals:job['delivered']=True;return json.loads(job['payload'])
            if identifier in self.consumed_jobs:return {'state':'consumed','reason':job['reason']}
            future=job['future']
            return {'state':'running' if future.running() else 'queued','cancel_requested':job['cancel'].is_set(),'progress':job.get('progress')}
        if operation=='job_cancel':
            self.reap_jobs();identifier=params['job_id'];job=self.owned_job(identifier)
            if 'future' in job:job['cancel'].set();job['future'].cancel()
            elif identifier in self.terminals:
                self.terminal_bytes-=len(job['payload']);job['payload']=b'{"state":"cancelled"}';self.terminal_bytes+=len(job['payload'])
                self.discard_artifacts(lambda key:self.artifact_jobs.get(key)==identifier)
            return {'cancel_requested':True}
        if operation in ['capabilities','wcs','view_link','sky_point','catalogue','xmatch']:return getattr(self,operation)(params)
        if operation in ['dependency_probe','dependency_install']:
            from fitsviewer_environment import probe,install
            return {'dependency_probe':probe,'dependency_install':install}[operation](self,params)
        if operation=='identify_stars':
            from fitsviewer_catalogue_services import identify
            return identify(self,params)
        if operation in ['service_presets','archive_search','vo_datalink']:
            from fitsviewer_archive_services import presets,search,datalink
            return {'service_presets':presets,'archive_search':search,'vo_datalink':datalink}[operation](self,params)
        if operation=='spectral_candidates':
            from fitsviewer_spectra import detect
            return detect(self,params)
        if operation=='spectral_analysis':
            from fitsviewer_spectral_analysis import analyze
            return analyze(self,params)
        if operation=='image_flux_match':
            from fitsviewer_image_matching import match_flux
            return match_flux(self,params)
        if operation=='gaia_solve':
            from fitsviewer_gaia import solve
            return solve(self,params)
        if operation=='samp':
            from fitsviewer_samp import request
            return request(self,params)
        if operation in ['vo_query','vo_download','xmatch_cds']:
            from fitsviewer_vo import request
            return request(self,operation,params)
        if operation in ['detect_stars','detect_lines','trace_curve','region_transform','line_profile']:
            from fitsviewer_measure import request
            return request(self,operation,params)
        if operation in ['export','rgb','math_preview']:
            from fitsviewer_export import request
            return request(self,operation,params)
        if operation=='reproject':return self.reproject(params)
        if operation=='release_pro':
            response=self.dispatch('release_pro_owner',params)
            return dict(response,wcs_cache_retained_for_minimal_j2000=len(self.headers))
        raise ValueError('Unknown science operation: '+operation)

    def run_job(self,operation,params,cancel):
        require_operation(operation,params)
        self.scope.owner=params.get('_viewer_session','cli');self.scope.operation=operation
        self.scope.file_capability=params.get('_file_capability') is True
        self.scope.document_generation=params.get('_document_generation',0);self.scope.document_explicit='_document_generation' in params;self.scope.gaia_generation=params.get('_gaia_generation')
        self.scope.science_role=params.get('_science_role','gaia' if operation=='gaia_solve' else 'pro');self.scope.role_epoch=params.get('_role_epoch')
        self.scope.job=params.get('_job_id') or uuid.uuid4().hex;self.scope.cancel=cancel
        try:
            if cancel.is_set():raise InterruptedError('Cancelled')
            result=self.reproject(params,cancel) if operation=='reproject' else self.dispatch(operation,dict(params,_cancel=cancel))
            if cancel.is_set():raise InterruptedError('Cancelled')
            return result
        except BaseException:
            job=self.scope.job
            self.discard_artifacts(lambda identifier:self.artifact_jobs.get(identifier)==job)
            raise
        finally:
            self.scope.cancel=None;self.scope.job=None

    def close(self):
        for job in self.jobs.values():job['cancel'].set();job['future'].cancel()
        if self.samp or getattr(self,'samp_http',None):
            try:
                from fitsviewer_samp import disconnect
                disconnect(self)
            except Exception:pass
        self.discard_artifacts(lambda identifier:True)
        with self.cache_lock:
            self.tables.clear();self.table_owners.clear();self.headers.clear();self.header_owners.clear()
        self.terminals.clear();self.terminal_bytes=0;self.consumed_jobs.clear()
        self.executor.shutdown(wait=False,cancel_futures=True)

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--cache',type=Path,required=True);args=parser.parse_args()
    protocol_output=sys.stdout
    sys.stdout=sys.stderr
    worker=Worker(args.cache)
    try:
        for line in sys.stdin:
            identifier=None
            try:
                if len(line)>8*1024*1024:raise ValueError('Science request exceeds 8 MiB')
                request=json.loads(line);identifier=request.get('id');operation=request['operation']
                if operation=='shutdown':break
                with contextlib.redirect_stdout(sys.stderr):result=worker.dispatch(operation,request.get('params',{}))
                response={'id':identifier,'ok':True,'result':clean(result)}
            except Exception as error:response={'id':identifier,'ok':False,'error':str(error),'type':type(error).__name__,'error_category':getattr(error,'category','missing_dependency' if isinstance(error,ModuleNotFoundError) else 'operation')}
            print(json.dumps(response,ensure_ascii=False,allow_nan=False,separators=(',',':')),file=protocol_output,flush=True)
    finally:worker.close()

if __name__=='__main__':main()
