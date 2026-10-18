"""Opt-in small public-service queries; never upload user images/catalogues.

记录真实空结果/失败与接口链；只有 --download-small 才选择并验证小型公开产品。
"""
from pathlib import Path
import argparse,json,sys,time
parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);parser.add_argument('--services',nargs='*');parser.add_argument('--download-small',action='store_true');args=parser.parse_args()
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'web/python'));sys.dont_write_bytecode=True
from fitsviewer_science import Worker,clean
from fitsviewer_archive_services import ARCHIVE_PRESETS
from fitsviewer_catalogue_services import CDS_PRESETS
from fitsviewer_gaia import GAIA_SERVICES
from astropy.io import fits
from astropy.wcs import WCS
from astropy.table import Table
import numpy as np
args.output.mkdir(parents=True,exist_ok=True);worker=Worker(args.output/'cache');report={'checks':[],'scope':'small public metadata queries; no user data upload; no package installation'}
def run(name,operation,params):
    started=time.monotonic()
    try:
        result=worker.dispatch(operation,params)
        item={'name':name,'operation':operation,'completed':True,'status':result.get('status'),'rows':result.get('rows'),
              'columns':result.get('columns'),'truncated':result.get('truncated'),'transport':result.get('transport'),
              'prior_service_failures':result.get('prior_service_failures'),'products':result.get('products',[])[:3],
              'spatial_checks':result.get('spatial_checks'),
              'links':result.get('links',[])[:5],'uploaded_rows':result.get('uploaded_rows'),'ambiguous_source_rows':result.get('ambiguous_source_rows'),
              'kind':result.get('kind'),'bytes':result.get('bytes'),'compression':result.get('compression'),'validated_content':result.get('validated_content')}
    except Exception as error:
        result=None;item={'name':name,'operation':operation,'completed':False,'error':str(error),'error_type':type(error).__name__,'category':getattr(error,'category',None)}
    item['seconds']=round(time.monotonic()-started,3);report['checks'].append(clean(item));(args.output/'online-services.json').write_text(json.dumps(clean(report),ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps({key:value for key,value in item.items() if key not in ['products','links','columns','transport']},ensure_ascii=True),flush=True)
    return result
try:
    selected=args.services
    if not selected or 'gaia' in selected:
        for i,url in enumerate(GAIA_SERVICES):
            run('gaia-'+str(i+1),'vo_query',{'protocol':'tap','service':url,'adql':"SELECT TOP 1 source_id,ra,dec,phot_g_mean_mag FROM gaiadr3.gaia_source WHERE 1=CONTAINS(POINT('ICRS',ra,dec),CIRCLE('ICRS',132.825,11.8,.04)) AND phot_g_mean_mag BETWEEN 14 AND 18",'max_rows':1,'timeout':25})
        wcs=WCS(naxis=2);wcs.wcs.ctype=['RA---TAN','DEC--TAN'];wcs.wcs.crval=[132.85347448680665,11.801107015171727];wcs.wcs.crpix=[32.5,32.5];wcs.wcs.cdelt=[-1/3600,1/3600];wcs.wcs.radesys='ICRS'
        image=args.output/'public-test-position.fits';fits.PrimaryHDU(np.ones((64,64),dtype='f4'),header=wcs.to_header()).writeto(image,overwrite=True)
        for source in ['gaia_dr3','sdss_dr17']:run('identify-'+source,'identify_stars',{'path':str(image),'catalogue_source':source,'max_rows':50,'timeout':25})
    if not selected or 'cds' in selected:
        for preset in CDS_PRESETS:
            run(preset['id'],'xmatch_cds',{'coordinates':{'ra_deg':[132.85347448680665],'dec_deg':[11.801107015171727],'frame':'icrs'},'remote_catalogue':preset['id'],'radius_arcsec':3.,'timeout':25,'strategy':'all'})
    for preset in ARCHIVE_PRESETS:
        if selected and preset['id'] not in selected:continue
        ra,dec=(132.825,11.8) if preset['id'] in ['sdss_dr17','legacy_dr9'] else (180.,45.) if preset['id']=='astron' else (53.12275,-27.80544)
        value=run(preset['id'],'archive_search',{'service_id':preset['id'],'ra_deg':ra,'dec_deg':dec,'radius_deg':.01,'max_rows':3,'timeout':25})
        if value:
            if preset['id']=='mast':
                observation=next((row for row in value.get('products',[]) if row.get('observation_id') is not None),None)
                if observation:run('mast-observation-products','archive_search',{'service_id':'mast','observation_id':str(observation['observation_id']),'ra_deg':ra,'dec_deg':dec,'max_rows':5,'timeout':25})
            links=[row for row in value.get('products',[]) if row.get('download_kind')=='datalink' and row.get('access_url')]
            if links:run(preset['id']+'-datalink','vo_datalink',{'url':links[0]['access_url'],'max_rows':5,'timeout':25})
            direct=[row for row in value.get('products',[]) if row.get('download_kind')=='fits' and row.get('access_url')]
            if args.download_small and preset['id']=='sdss_dr17' and direct:
                row=direct[0];run('sdss-verified-small-download','vo_download',{'url':row['access_url'],'access_format':row.get('access_format'),'maximum_bytes':16*1024*1024,'timeout':60})
finally:worker.close()
report['completed_queries']=sum(item['completed'] for item in report['checks']);report['failed_queries']=sum(not item['completed'] for item in report['checks'])
(args.output/'online-services.json').write_text(json.dumps(clean(report),ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'completed':report['completed_queries'],'failed':report['failed_queries']}))
