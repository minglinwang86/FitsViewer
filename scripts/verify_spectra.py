"""Numerical/Specutils acceptance; optional real SDSS packet smoke test.

合成数据仅用于数值边界测试；展示及真实文件烟雾测试使用公开 SDSS 数据。
"""
import argparse
import json
import os
from pathlib import Path
import sys
import threading
import time

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'web/python'))
import fitsviewer_science as fs

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output',type=Path,required=True)
parser.add_argument('--core',type=Path)
parser.add_argument('--real-spectrum',type=Path)
args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
report={'checks':[]}
def check(name,operation):
    started=time.perf_counter()
    try:result=operation();report['checks'].append({'name':name,'passed':True,'seconds':time.perf_counter()-started,'detail':fs.clean(result)})
    except Exception as error:
        import traceback
        traceback.print_exc();report['checks'].append({'name':name,'passed':False,'error':str(error)})
    (args.output/'spectra-validation.json').write_text(json.dumps(report,indent=2,ensure_ascii=False),encoding='utf-8')

worker=fs.Worker(args.output/'cache')
def lazy():
    result=worker.capabilities({});assert not result['loaded_modules'],result
    assert 'spectral_candidates' in result['operations'] and result['packages']['specutils']
    return result
check('lazy_import_and_selected_environment',lazy)

import numpy as np
from fitsviewer_spectra import detect
rng=np.random.default_rng(20261018)
wave=np.linspace(4800,5100,1501)
flux=1.+0.0003*(wave-4900)+2.5*np.exp(-.5*((wave-4920.)/1.2)**2)-1.8*np.exp(-.5*((wave-4990.)/1.8)**2)+rng.normal(0,.025,len(wave))
errors=np.full(len(wave),.025)
class FixtureWorker(fs.Worker):
    def __init__(self,x=wave,y=flux,error=errors,meta=None):
        super().__init__(args.output/'fixture-cache')
        self.packet={'x':x,'values':y,'meta':{'spectrum':meta or {'x_label':'Wavelength','x_unit':''}}}
        if error is not None:self.packet['error']=error
    def dataset(self,params):return self.packet
params={'unit':'Angstrom','source':'numerical fixture','method':'derivative','sigma':5.,'continuum_window':101}

def truth(method):
    instance=FixtureWorker();result=detect(instance,dict(params,method=method));instance.close()
    emission=min((row for row in result['candidates'] if row['kind']=='emission'),key=lambda row:abs(row['center_angstrom']-4920.))
    absorption=min((row for row in result['candidates'] if row['kind']=='absorption'),key=lambda row:abs(row['center_angstrom']-4990.))
    assert abs(emission['center_angstrom']-4920.)<.5 and abs(absorption['center_angstrom']-4990.)<.5,result
    assert result['continuum']=='running median subtracted' and result['versions']['specutils']
    return {'emission':emission,'absorption':absorption,'count':len(result['candidates']),'versions':result['versions']}
check('derivative_emission_absorption_truth',lambda:truth('derivative'))
check('threshold_emission_absorption_truth',lambda:truth('threshold'))

def units_and_order():
    result=[]
    for name,factor,reverse in [('Angstrom',1.,False),('nm',10.,False),('um',10000.,False),('Angstrom',1.,True)]:
        x=wave/factor;y=flux;e=errors
        if reverse:x=x[::-1];y=y[::-1];e=e[::-1]
        instance=FixtureWorker(x,y,e);value=detect(instance,dict(params,unit=name));instance.close()
        line=min(value['candidates'],key=lambda row:abs(row['center_angstrom']-4920.))
        assert abs(line['center_angstrom']-4920.)<.5
        assert abs(float(x[line['sample_index']])*factor-line['center_angstrom'])<1e-8,line
        result.append({'unit':name,'descending':reverse,'center_angstrom':line['center_angstrom']})
    return result
check('unit_equivalence_and_descending_indices',units_and_order)

def rejects():
    cases=[(FixtureWorker(),dict(params,unit='pixel')),(FixtureWorker(),dict(params,sigma=float('nan'))),
           (FixtureWorker(),dict(params,continuum_window=4)),(FixtureWorker(error=None),dict(params,method='threshold')),
           (FixtureWorker(meta={'x_label':'Sample','x_unit':'pixel'}),params),
           (FixtureWorker(meta={'x_label':'Frequency','x_unit':'GHz'}),params),
           (FixtureWorker(np.ones(len(wave))),params)]
    messages=[]
    for instance,options in cases:
        try:detect(instance,options)
        except (ValueError,RuntimeError) as error:messages.append(str(error))
        else:raise AssertionError('Invalid input was accepted')
        finally:instance.close()
    return messages
check('invalid_units_axes_noise_and_duplicate_coordinates',rejects)

def missing_api_message():
    import builtins
    from unittest.mock import patch
    original=builtins.__import__;instance=FixtureWorker()
    def legacy(name,globals=None,locals=None,fromlist=(),level=0):
        if name=='specutils' and 'Spectrum' in (fromlist or ()):raise ImportError('mock legacy API')
        return original(name,globals,locals,fromlist,level)
    try:
        with patch('builtins.__import__',side_effect=legacy):
            try:detect(instance,params)
            except ModuleNotFoundError as error:
                message=str(error);assert 'Specutils >=2,<3' in message and "python -m pip install 'specutils>=2,<3'" in message
                return message
            raise AssertionError('Missing API was accepted')
    finally:instance.close()
check('legacy_specutils_api_has_selected_environment_install_hint',missing_api_message)

def gaps():
    y=flux.copy();y[(wave>4918)&(wave<4922)]=np.nan
    instance=FixtureWorker(y=y,error=None);result=detect(instance,params);instance.close()
    assert result['samples_used']<len(wave)
    assert all(not 4918<row['center_angstrom']<4922 for row in result['candidates'])
    return {'samples_used':result['samples_used'],'candidate_count':len(result['candidates'])}
check('masked_gap_and_estimated_noise',gaps)

def background():
    instance=FixtureWorker()
    start=instance.dispatch('job_start',{'operation':'spectral_candidates','params':params,'_viewer_session':'spectra-test'})
    deadline=time.time()+30
    while time.time()<deadline:
        status=instance.dispatch('job_status',{'job_id':start['job_id'],'_viewer_session':'spectra-test'})
        if status['state'] not in ('running','queued'):break
        time.sleep(.02)
    assert status['state']=='completed',status
    instance.dispatch('release_pro_owner',{'_viewer_session':'spectra-test'});assert not instance.jobs
    instance.close();return {'state':status['state'],'candidates':len(status['result']['candidates']),'released':True}
check('background_job_and_owner_release',background)

def cancel_queued():
    instance=FixtureWorker();gate=threading.Event();instance.executor.submit(lambda:gate.wait(5))
    try:
        start=instance.dispatch('job_start',{'operation':'spectral_candidates','params':params,'_viewer_session':'cancel-test'})
        instance.dispatch('job_cancel',{'job_id':start['job_id'],'_viewer_session':'cancel-test'})
        status=instance.dispatch('job_status',{'job_id':start['job_id'],'_viewer_session':'cancel-test'})
        assert status['state']=='cancelled',status
        instance.dispatch('release_pro_owner',{'_viewer_session':'cancel-test'});assert not instance.jobs
        return status
    finally:gate.set();instance.close()
check('cancel_queued_spectral_job',cancel_queued)

if args.real_spectrum:
    def real():
        if not args.core:raise ValueError('--core is required for the real shared-packet test')
        os.environ['FITSVIEWER_CORE_BINARY']=str(args.core.resolve())
        value=worker.dispatch('spectral_candidates',{'path':str(args.real_spectrum.resolve()),'hdu':1,'unit':'Angstrom','source':'public SDSS smoke test',
                                                    'method':'derivative','sigma':5.,'continuum_window':101})
        assert value['samples_used']>=16 and value['candidates']
        return {'path':str(args.real_spectrum),'count':len(value['candidates']),'samples_used':value['samples_used'],'versions':value['versions'],
                'candidate_preview':value['candidates'][:24],'note':'No truth labels or redshift solution inferred from pipeline metadata.'}
    check('real_sdss_shared_core_packet',real)
worker.close()
print(json.dumps({'passed':sum(row['passed'] for row in report['checks']),'checks':len(report['checks']),'output':str(args.output)}))
raise SystemExit(0 if all(row['passed'] for row in report['checks']) else 1)
