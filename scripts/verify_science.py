"""Independent numerical/protocol checks for optional external science adapters."""
from pathlib import Path
import argparse,json,sys,tempfile,time,threading,io,urllib.request
parser=argparse.ArgumentParser();parser.add_argument('--output',type=Path,required=True);parser.add_argument('--core',type=Path);args=parser.parse_args()
root=Path(__file__).resolve().parents[1];sys.path.insert(0,str(root/'web/python'))
if args.core:
    import os
    os.environ['FITSVIEWER_CORE_BINARY']=str(args.core.resolve())
import fitsviewer_science as fs
args.output.mkdir(parents=True,exist_ok=True)
report={'checks':[],'limitations':[]}
def check(name,function):
    started=time.perf_counter()
    try:detail=function();report['checks'].append({'name':name,'passed':True,'seconds':time.perf_counter()-started,'detail':fs.clean(detail)})
    except Exception as error:
        import traceback
        traceback.print_exc();report['checks'].append({'name':name,'passed':False,'error':str(error)})
    (args.output/'science-validation.json').write_text(json.dumps(report,ensure_ascii=False,indent=2),encoding='utf-8')
worker=fs.Worker(args.output/'cache')
check('lazy_start',lambda: {'loaded':worker.capabilities({})['loaded_modules']} if not worker.capabilities({})['loaded_modules'] else (_ for _ in ()).throw(AssertionError('Scientific module imported eagerly')))
import numpy as np
from astropy.io import fits
from astropy.wcs import WCS
from astropy.table import Table
rng=np.random.default_rng(20261017);y,x=np.mgrid[:96,:128];background=20.
star=background+rng.normal(0,1,x.shape)+500*np.exp(-((x-42.3)**2+(y-53.7)**2)/(2*1.4**2))
wcs=WCS(naxis=2);wcs.wcs.ctype=['RA---TAN','DEC--TAN'];wcs.wcs.crval=[359.999,60.];wcs.wcs.crpix=[64.5,48.5];wcs.wcs.cd=np.array([[-.2,.1],[.1,.2]])/3600;wcs.wcs.radesys='FK5';wcs.wcs.equinox=2000
header=wcs.to_header();header['BUNIT']='adu'
path=args.output/'sky.fits';fits.PrimaryHDU(star.astype('float32'),header=header).writeto(path,overwrite=True)
params={'path':str(path),'hdu':0}
def wcs_test():
    points=[[63.5,47.5],[0.,0.],[127.,95.]];result=worker.wcs(dict(params,points=points))
    assert abs(result['points'][0]['ra_deg']-359.999)<1e-9 and abs(result['points'][0]['dec_deg']-60)<1e-9
    from astropy.wcs.utils import pixel_to_skycoord
    from astropy.coordinates import FK5
    from astropy.time import Time
    reference=pixel_to_skycoord(np.asarray(points)[:,0],np.asarray(points)[:,1],wcs).transform_to(FK5(equinox=Time('J2000')))
    assert np.max(np.abs(np.array([p['ra_deg'] for p in result['points']])-reference.ra.deg))<1e-10
    return result
check('j2000_rotated_ra_wrap',wcs_test)
def stars_test():
    result=worker.dispatch('detect_stars',dict(params,threshold_sigma=8,fwhm=3.3));source=min(result['sources'],key=lambda s:(s['x']-42.3)**2+(s['y']-53.7)**2)
    error=np.hypot(source['x']-42.3,source['y']-53.7);assert error<.08,(error,result);return {'sources':len(result['sources']),'centroid_error_pixels':error,'source':source}
check('star_centroid_truth',stars_test)
def catalogue_test():
    sky=worker.wcs(dict(params,points=[[42.3,53.7],[63.5,47.5]]))['points'];table=Table({'id':['target','center'],'ra':[p['ra_deg'] for p in sky],'dec':[p['dec_deg'] for p in sky]})
    results=[]
    for suffix,format in [('.csv','ascii.csv'),('.vot','votable'),('.fits','fits')]:
        cat=args.output/('catalogue'+suffix);table.write(cat,format=format,overwrite=True)
        value=worker.catalogue(dict(params,catalogue=str(cat),ra_column='ra',dec_column='dec',frame='fk5',id_column='id'));assert value['rows']==2 and len(value['markers'])==2
        assert np.hypot(value['markers'][0]['x']-42.3,value['markers'][0]['y']-53.7)<1e-6;results.append({'format':format,'markers':value['markers']})
    return results
check('csv_votable_fits_catalogues',catalogue_test)
def xmatch_test():
    value=worker.xmatch({'first':{'ra_deg':[359.9999,10.,None,0.0001],'dec_deg':[60.,0.,0.,60.]},'second':{'ra_deg':[0.,0.0001,50.],'dec_deg':[60.,60.,1.]},'radius_arcsec':1.,'strategy':'all'})
    assert len(value['matches'])==4 and value['unmatched_first']==[1] and value['invalid_first']==[2] and all(p['ambiguous'] for p in value['matches']),value
    return value
check('spherical_xmatch_wrap_invalid_many_to_one',xmatch_test)
trace=background+rng.normal(0,.5,x.shape)+80*np.exp(-(y-(25.+.0015*(x-64)**2))**2/(2*1.5**2))
tracepath=args.output/'trace.fits';fits.PrimaryHDU(trace.astype('float32'),header=header).writeto(tracepath,overwrite=True)
def trace_test():
    value=worker.dispatch('trace_curve',{'path':str(tracepath),'seed':[3.,30.6],'search_half_width':5.,'order':2,'threshold_sigma':4.})
    points=np.asarray(value['points']);rms=np.sqrt(np.mean((points[:,1]-(25.+.0015*(points[:,0]-64)**2))**2));assert len(points)>100 and rms<.1,(rms,len(points))
    return {'truth_rms_pixels':rms,'points':len(points),'reported_rms':value['rms_pixels']}
check('seeded_curved_spectrum_trace',trace_test)
def line_test():
    line=background+rng.normal(0,.3,x.shape)+60*np.exp(-(y-(20.+.25*x))**2/(2*.7**2));linepath=args.output/'line.fits';fits.PrimaryHDU(line.astype('float32'),header=header).writeto(linepath,overwrite=True)
    value=worker.dispatch('detect_lines',{'path':str(linepath),'min_length':50,'threshold_sigma':4,'seed':20261017})
    assert value['lines'] and min(abs(l['angle_deg']%180-np.degrees(np.arctan(.25))) for l in value['lines'])<3,value
    return value
check('straight_trace_truth',line_test)
def transform_test():
    template={'schema':1,'source_header':header.tostring(sep='\n'),'source_dimensions':[128,96],'source_hdu':0,'apertures':[{'name':'C','shape':{'kind':'circle','center':[42.3,53.7],'radius':5.},'background':{'kind':'ring','center':[42.3,53.7],'inner':8.,'outer':12.},'exclude':[]}]}
    value=worker.dispatch('region_transform',dict(params,template=template));vertices=np.asarray(value['apertures'][0]['shape']['points']);r=np.linalg.norm(vertices-[42.3,53.7],axis=1);assert np.max(abs(r-5))<1e-6
    assert len(value['apertures'][0]['background']['holes'])==1
    return {'vertices':len(vertices),'radial_error':float(np.max(abs(r-5)))}
check('sky_region_boundary_identity_hole',transform_test)
def profile_test():
    value=worker.dispatch('line_profile',{'path':str(tracepath),'points':[[2,30.766],[64,25],[124,30.4]],'width':5.,'background_width':4.});assert len(value['distance_pixels'])>120 and np.nanmedian(value['boxcar_net'])>150;return {'samples':len(value['distance_pixels']),'median_boxcar':np.nanmedian(value['boxcar_net'])}
check('finite_width_trace_profile',profile_test)
def reproject_test():
    value=worker.reproject(dict(params,ra_deg=359.999,dec_deg=60.,scale_arcsec=.22360679775,width=80,height=70))
    with fits.open(value['_path']) as f:
        assert f[0].data.shape==(70,80) and np.isfinite(f[0].data).sum()>4000
    return value
check('bounded_visualization_reprojection',reproject_test)
def exports_test():
    from PIL import Image
    import importlib.util,re
    output=[]
    for format in ['png','jpg','svg','pdf']:
        result=worker.dispatch('export',dict(params,format=format,width=800,height=600,dpi=150,wcs_axes=True,compass=True,scale_arcsec=3.,title='科研图像',annotations=[{'kind':'text','x':10,'y':10,'text':r'$F_\lambda / \sigma$'}],view={'low':18.,'high':80.,'stretch':'Log','render':{'palette':{'name':'Heat'}}}))
        file=Path(result['_path']);assert file.stat().st_size>1000
        if format in ['png','jpg']:assert Image.open(file).size==(800,600)
        if format=='pdf':
            assert file.read_bytes().startswith(b'%PDF-')
            if importlib.util.find_spec('pypdf'):
                from pypdf import PdfReader
                box=PdfReader(file).pages[0].mediabox;page_width,page_height=float(box.width),float(box.height)
            else:
                # Matplotlib writes a plain page MediaBox. pypdf is an optional
                # verifier, not an export runtime dependency. / 验证不强制额外装包。
                match=re.search(rb'/MediaBox\s*\[\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*\]',file.read_bytes())
                assert match,'PDF has no readable page MediaBox';page_width,page_height=map(float,match.groups())
            assert abs(page_width-800/150*72)<.01 and abs(page_height-600/150*72)<.01
        if format=='svg':assert '<path' in file.read_text(encoding='utf-8')
        output.append(result)
    return output
check('png_jpg_pdf_svg_physical_sizes_math_cjk',exports_test)
def rgb_test():
    value=worker.dispatch('rgb',{'channels':[dict(params,low=18,high=80)]*3});assert value['width']==128 and value['height']==96 and value['reprojected_channels']==[]
    return value
check('rgb_verified_wcs_channels',rgb_test)
def samp_test():
    import os
    from astropy.samp import SAMPHubServer,SAMPIntegratedClient
    hub=SAMPHubServer(addr='127.0.0.1',web_profile=False,mode='multiple',lockfile=str(args.output/'samp-test.lock'));hub.start()
    previous_hub=os.environ.get('SAMP_HUB');os.environ['SAMP_HUB']='std-lockurl:'+(args.output/'samp-test.lock').resolve().as_uri()
    client=SAMPIntegratedClient(addr='127.0.0.1',name='verification peer');client.connect(hub=hub)
    try:
        worker.dispatch('samp',{'action':'connect'})
        reply=client.call_and_wait(worker.samp.get_public_id(),{'samp.mtype':'coord.pointAt.sky','samp.params':{'ra':'359.999','dec':'60.'}},10)
        assert reply['samp.status']=='samp.ok'
        events=worker.dispatch('samp',{'action':'events'})['events'];assert events[-1]['mtype']=='coord.pointAt.sky'
        from fitsviewer_samp import file_url
        with urllib.request.urlopen(file_url(worker,path),timeout=5) as response:assert response.read()==path.read_bytes()
        worker.dispatch('samp',{'action':'disconnect'});assert worker.samp is None and worker.samp_http is None
        return {'hub':'Astropy Standard Profile','call_reply':reply,'events':events,'file_transfer_verified':True}
    finally:
        client.disconnect();hub.stop()
        if previous_hub is None:os.environ.pop('SAMP_HUB',None)
        else:os.environ['SAMP_HUB']=previous_hub
check('real_samp_hub_call_reply_file_cleanup',samp_test)
worker.close()
report['passed']=all(c['passed'] for c in report['checks']);report['external_packages']=worker.capabilities({})['packages'];report['limitations']=['TOPCAT/Aladin and real Mac UI need their own application/platform checks; Hub protocol tests alone do not establish those.']
(args.output/'science-validation.json').write_text(json.dumps(fs.clean(report),ensure_ascii=False,indent=2),encoding='utf-8')
print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'failures':[c['name'] for c in report['checks'] if not c['passed']]}))
raise SystemExit(0 if report['passed'] else 1)

