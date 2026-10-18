"""Scientific/service boundaries without installs or public uploads.

真实单位/WCS数值、重复匹配、VO字段语义与恶意下载边界；在线可用性另行小查询。
"""
from pathlib import Path
import bz2,gzip,io,json,sys,tempfile,threading,unittest
from types import SimpleNamespace
from unittest.mock import patch

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'web/python'))
import fitsviewer_science as science
import fitsviewer_environment as environment
import fitsviewer_archive_services as archives
import fitsviewer_network as network
import fitsviewer_vo as vo
from astropy.table import Table
from astropy.io import fits
from astropy.io.votable import from_table,writeto
from astropy.wcs import WCS
import numpy as np


def votable(table):
    stream=io.BytesIO();writeto(from_table(table),stream);return stream.getvalue()


class Response(io.BytesIO):
    def __init__(self,data,mime='application/octet-stream',length=None):
        super().__init__(data);self.headers={'Content-Type':mime,'Content-Length':str(len(data) if length is None else length)}
    def geturl(self):return 'https://public.example/product'


class Services(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='fitsviewer-services-',dir=ROOT/'qa');self.path=Path(self.temp.name)
        self.worker=science.Worker(self.path/'cache')

    def tearDown(self):self.worker.close();self.temp.cleanup()

    def image(self):
        wcs=WCS(naxis=2);wcs.wcs.ctype=['RA---TAN','DEC--TAN'];wcs.wcs.crval=[359.999,60.]
        wcs.wcs.crpix=[50.5,40.5];wcs.wcs.cd=np.array([[-1.,.2],[.2,1.]])/3600;wcs.wcs.radesys='ICRS'
        path=self.path/'field.fits';fits.PrimaryHDU(np.ones((80,100),dtype='f4'),header=wcs.to_header()).writeto(path)
        return path,wcs

    def test_identification_projects_ids_magnitudes_ra_wrap_and_current_override(self):
        from astropy.wcs.utils import pixel_to_skycoord
        path,wcs=self.image();truth=np.array([[10.2,20.3],[80.1,59.7],[300,300]])
        sky=pixel_to_skycoord(truth[:,0],truth[:,1],wcs,origin=0).icrs
        table=Table({'source_id':[111,222,333],'ra':sky.ra.deg,'dec':sky.dec.deg,'phot_g_mean_mag':[14.2,15.1,16.]})
        with patch('fitsviewer_gaia.reference_table',return_value=(table,'Gaia test identities',None)):
            result=self.worker.dispatch('identify_stars',{'path':str(path),'catalogue_source':'gaia_dr3'})
        self.assertEqual([m['id'] for m in result['markers']],['111','222'])
        self.assertEqual([m['magnitude'] for m in result['markers']],[14.2,15.1])
        self.assertLess(np.max(abs(np.array([[m['x'],m['y']] for m in result['markers']])-truth[:2])),1e-6)
        shifted=wcs.to_header();shifted['CRVAL1']+=.002
        with patch('fitsviewer_gaia.reference_table',return_value=(table,'Gaia test identities',None)):
            changed=self.worker.dispatch('identify_stars',{'path':str(path),'wcs_override':shifted.tostring(sep='\n')})
        self.assertGreater(abs(changed['markers'][0]['x']-result['markers'][0]['x']),1.)
        self.assertEqual(changed['coordinate_origin'],0);self.assertIn('override',changed['wcs_source'])

    def test_identification_without_wcs_fails_before_network(self):
        path=self.path/'no-wcs.fits';fits.PrimaryHDU(np.ones((40,40),dtype='f4')).writeto(path)
        with patch('fitsviewer_gaia.reference_table') as query:
            with self.assertRaisesRegex(ValueError,'WCS'):self.worker.dispatch('identify_stars',{'path':str(path)})
            query.assert_not_called()

    def test_empty_catalogue_is_distinct_from_service_error(self):
        path,_=self.image();table=Table(names=['source_id','ra','dec','phot_g_mean_mag'],dtype=['i8','f8','f8','f8'])
        with patch('fitsviewer_gaia.reference_table',return_value=(table,'Gaia',None)):
            result=self.worker.dispatch('identify_stars',{'path':str(path)})
        self.assertEqual(result['status'],'empty');self.assertEqual(result['markers'],[])

    def test_cds_invalid_file_and_columns_do_not_upload(self):
        for params,pattern in [({'catalogue':''},'local coordinate'),({'catalogue':'x'},'RA and Dec')]:
            with self.assertRaisesRegex(ValueError,pattern):vo.xmatch(self.worker,dict(params,remote_catalogue='vizier:I/355/gaiadr3'))
        path=self.path/'cat.csv';Table({'alpha':[1.],'delta':[2.]}).write(path,format='ascii.csv')
        with self.assertRaisesRegex(ValueError,'absent'):vo.xmatch(self.worker,{'catalogue':str(path),'ra_column':'ra','dec_column':'dec','remote_catalogue':'vizier:I/355/gaiadr3'})

    def test_cds_reused_coordinates_keep_ambiguity_after_nearest_selection(self):
        result=Table({'fv_source_row':[0,0,2],'angDist':[.2,.1,.3],'Source':[10,11,12]})
        with patch('astroquery.xmatch.XMatchClass.query',return_value=result):
            value=vo.xmatch(self.worker,{'coordinates':{'ra_deg':[1.,np.nan,3.],'dec_deg':[2.,0.,4.],'frame':'icrs'},
                                         'remote_catalogue':'vizier:I/355/gaiadr3','radius_arcsec':1.,'strategy':'nearest'})
        self.assertEqual(value['rows'],2);self.assertEqual(value['candidate_rows'],3);self.assertEqual(value['ambiguous_source_rows'],[0])
        self.assertEqual(value['invalid_rows_excluded'],1);self.assertEqual(value['preview']['Source'],[11,12])
        self.assertEqual(value['preview']['fv_ambiguous'],[True,False])

    def test_cds_invalid_declination_excluded_before_upload(self):
        result=Table(names=['fv_source_row','angDist'],dtype=['i8','f8'])
        with patch('astroquery.xmatch.XMatchClass.query',return_value=result) as query:
            value=vo.xmatch(self.worker,{'coordinates':{'ra_deg':[1.,3.],'dec_deg':[100.,4.]},'remote_catalogue':'vizier:I/355/gaiadr3'})
        self.assertEqual(value['invalid_rows_excluded'],1);self.assertEqual(len(query.call_args.kwargs['cat1']),1)

    def test_votable_field_names_utf16_misdeclaration_overflow_and_errors(self):
        body=votable(Table({'access_url':['https://example.invalid/data.fits'],'access_format':['image/fits']}))
        body=body.replace(b'ID="access_url"',b'ID="col_0"').replace(b'ID="access_format"',b'ID="col_1"')
        body=body.replace(b'encoding="utf-8"',b'encoding="utf-16"')
        table,meta=network.parse_votable(body)
        self.assertIn('access_url',table.colnames);self.assertEqual(meta['field_id_names']['col_0'],'access_url');self.assertTrue(meta['xml_declaration_normalized'])
        error=body.replace(b'<RESOURCE type="results">',b'<RESOURCE type="results"><INFO name="QUERY_STATUS" value="ERROR">bad query</INFO>')
        with self.assertRaisesRegex(network.ServiceError,'bad query'):network.parse_votable(error)
        with self.assertRaises(network.ServiceError):network.parse_votable(b'<html>login</html>')

    def test_obscore_sizes_and_product_semantics(self):
        table=Table({'obs_id':['science','weight'],'access_url':['https://example.invalid/image.fits','https://example.invalid/links'],
                     'access_format':['image/fits','application/x-votable+xml;content=datalink'],'access_estsize':[12.,7.],
                     'dataproduct_type':['image','image'],'prodtype':['science','weight'],'obs_collection':['DES','DES']})
        products=archives.product_rows(table,archives.ARCHIVE_PRESETS[2]);self.assertEqual(products[0]['estimated_bytes'],12000)
        self.assertEqual(products[1]['download_kind'],'datalink');self.assertEqual(products[1]['semantics'],'weight')
        broken=Table({'obs_id':['ALMA'],'access_url':['https://example.invalid/datalink/sync?ID=a'],'access_format':['applicati']})
        product=archives.product_rows(broken,archives.ARCHIVE_PRESETS[-2])[0]
        self.assertEqual(product['download_kind'],'datalink');self.assertIn('malformed',product['format_note'])

    def test_archive_nonempty_table_requires_product_identity_schema(self):
        with self.assertRaisesRegex(network.ServiceError,'identity'):archives.product_rows(Table({'unrelated':[1]}),archives.ARCHIVE_PRESETS[2])

    def test_spherical_coverage_checks_ra_wrap_pole_and_edge_intersection(self):
        polygon='POLYGON ICRS 359.9 -0.1 0.1 -0.1 0.1 0.1 359.9 0.1'
        self.assertTrue(archives.footprint_check(polygon,0.,0.,.01));self.assertFalse(archives.footprint_check(polygon,180.,0.,.01))
        self.assertTrue(archives.footprint_check(polygon,.15,0.,.06));self.assertFalse(archives.footprint_check(polygon,.2,0.,.01))
        pole='POLYGON ICRS 0 89 90 89 180 89 270 89'
        self.assertTrue(archives.footprint_check(pole,45,89.9,.01));self.assertFalse(archives.footprint_check(pole,45,85,.01))
        self.assertTrue(archives.footprint_check('CIRCLE ICRS 0.01 0 0.01',359.99,0,.011))
        self.assertIsNone(archives.footprint_check('UNION ICRS ( CIRCLE 1 2 3 )',1,2,.01))

    def test_mast_collection_json_preserves_nm_units_and_download_product_uri(self):
        preset=next(item for item in archives.ARCHIVE_PRESETS if item['id']=='mast')
        response={'status':'COMPLETE','fields':[{'name':'obs_id','type':'string'},{'name':'obsid','type':'int'},{'name':'em_min','type':'float'},
                                             {'name':'dataURL','type':'string'},{'name':'s_region','type':'string'}],
                  'data':[{'obs_id':'hst_obs','obsid':123,'em_min':600.,'dataURL':'mast:HST/product/a.fits','s_region':'CIRCLE ICRS 1 2 0.1'}]}
        with patch.object(archives,'fetch_bytes',return_value=(json.dumps(response).encode(),{'encoding':'utf-8'})) as fetch:
            result=archives.mast_search(self.worker,{'collection':'HST'},preset,1.,2.,.01,5)
        self.assertAlmostEqual(result['products'][0]['wavelength_min_m'],6e-7)
        self.assertTrue(result['products'][0]['product_query_available']);self.assertEqual(result['products'][0]['coverage_check'],'overlaps_query')
        self.assertIn('uri=mast%3AHST%2Fproduct%2Fa.fits',result['products'][0]['access_url'])
        self.assertEqual(json.loads(fetch.call_args.kwargs['data']['request'])['service'],'Mast.Caom.Filtered.Position')

    def test_datalink_does_not_download_errors_services_or_recursive_links(self):
        table=Table({'ID':['a','a','a','a'],'access_url':['https://example.invalid/a.fits','','https://example.invalid/again',''],
                     'semantics':['#this','#proc','#auxiliary','#error'],'error_message':['','','','Permission denied'],
                     'service_def':['','cutout','',''],'content_type':['image/fits','','application/x-votable+xml;content=datalink',''],
                     'content_length':[2048,0,200,0]})
        with patch.object(archives,'fetch_bytes',return_value=(votable(table),{})):
            value=archives.datalink(self.worker,{'url':'https://example.invalid/links'})
        self.assertTrue(value['links'][0]['downloadable']);self.assertFalse(value['links'][1]['downloadable'])
        self.assertTrue(value['links'][1]['requires_service']);self.assertTrue(value['links'][2]['recursive_datalink']);self.assertFalse(value['links'][3]['downloadable'])

    def test_download_fits_bzip_gzip_and_content_validation(self):
        stream=io.BytesIO();fits.PrimaryHDU(np.ones((3,4),dtype='f4')).writeto(stream);body=stream.getvalue()
        for packed,compression in [(body,None),(gzip.compress(body),'gzip'),(bz2.compress(body),'bzip2')]:
            with patch.object(vo.urllib.request,'urlopen',return_value=Response(packed)):
                result=vo.download(self.worker,{'url':'https://example.invalid/no-extension','maximum_bytes':10000})
            self.assertEqual(result['kind'],'fits');self.assertEqual(result['compression'],compression);self.assertEqual(Path(result['_path']).read_bytes(),body)
        for body,mime in [(b'<html>login</html>','text/html'),(b'PK\x03\x04archive','application/zip'),(b'not fits','application/fits')]:
            with patch.object(vo.urllib.request,'urlopen',return_value=Response(body,mime)):
                with self.assertRaises((network.ServiceError,ValueError)):vo.download(self.worker,{'url':'https://example.invalid/a.fits'})

    def test_download_decompression_size_bomb_rejected(self):
        packed=gzip.compress(b'SIMPLE  ='+b' '*100000)
        with patch.object(vo.urllib.request,'urlopen',return_value=Response(packed)):
            with self.assertRaisesRegex(ValueError,'Decompressed'):vo.download(self.worker,{'url':'https://example.invalid/a.gz','maximum_bytes':1000})
        self.assertFalse(list(self.worker.cache.glob('*.part')));self.assertFalse(list(self.worker.cache.glob('*.decoded')))

    def test_download_truncated_response_rejected(self):
        with patch.object(vo.urllib.request,'urlopen',return_value=Response(b'123','text/plain',length=40)):
            with self.assertRaisesRegex(ValueError,'Content-Length'):vo.download(self.worker,{'url':'https://example.invalid/a.txt'})

    def test_timeout_retries_once_and_cancellation_does_not_start(self):
        import requests
        with patch.object(network,'session',side_effect=requests.ReadTimeout('test deadline')) as transport:
            with self.assertRaisesRegex(network.ServiceError,'one retry'):network.fetch_bytes('https://example.invalid')
            self.assertEqual(transport.call_count,2)
        cancel=threading.Event();cancel.set()
        with patch.object(network,'session') as transport:
            with self.assertRaises(InterruptedError):network.fetch_bytes('https://example.invalid',cancel=cancel)
            transport.assert_not_called()

    def test_dependency_whitelist_rejects_options_urls_and_versions(self):
        for package in ['--upgrade','numpy==0.1','https://example.invalid/a.whl','pypdf']:
            with self.assertRaisesRegex(ValueError,'supported package'):environment.install(self.worker,{'packages':[package]})

    def test_dependency_resolution_blocks_replacing_existing_package(self):
        def resolve(worker,args,log,params,stage,timeout):
            Path(args[args.index('--report')+1]).write_text(json.dumps({'install':[{'metadata':{'name':'numpy','version':'1.0'}},{'metadata':{'name':'astroalign','version':'2.6.0'}}]}));return 0
        with patch.object(environment,'installed',return_value={'numpy':'2.1.3'}),patch.object(environment,'run',side_effect=resolve) as command:
            result=environment.install(self.worker,{'packages':['astroalign']})
        self.assertEqual(result['status'],'blocked_conflict');self.assertEqual(command.call_count,1);self.assertTrue(result['preexisting_packages_preserved'])

    def test_dependency_install_fixes_plan_and_disables_dependency_reresolution(self):
        calls=[]
        def execute(worker,args,log,params,stage,timeout):
            calls.append(args)
            if '--report' in args:Path(args[args.index('--report')+1]).write_text(json.dumps({'install':[{'metadata':{'name':'astroalign','version':'2.6.0'}}]}))
            return 0
        with patch.object(environment,'installed',side_effect=[{'numpy':'2.1.3'},{'numpy':'2.1.3'},{'numpy':'2.1.3','astroalign':'2.6.0'}]),patch.object(environment,'run',side_effect=execute),patch.object(environment,'dependency_conflicts',return_value=[]):
            result=environment.install(self.worker,{'packages':['astroalign']})
        self.assertEqual(result['status'],'installed');self.assertIn('--no-deps',calls[1]);self.assertIn('astroalign==2.6.0',calls[1]);self.assertNotIn('--upgrade',calls[1])

    def test_dependency_reverse_constraints_block_new_conflicts(self):
        distribution=SimpleNamespace(metadata={'Name':'existing-science-package'},requires=['new-helper<2'])
        with patch.object(environment.importlib.metadata,'distributions',return_value=[distribution]):
            conflicts=environment.dependency_conflicts({'existing-science-package':'1.0'},[{'metadata':{'name':'new-helper','version':'3.0'}}])
        self.assertEqual(conflicts[0]['requires'],'new-helper<2')

    def test_environment_busy_does_not_consume_completed_results(self):
        from concurrent.futures import Future
        running=Future();running.set_running_or_notify_cancel();done=Future();done.set_result({'kept':True})
        self.worker.jobs={'one':{'future':running,'operation':'dependency_install'},'two':{'future':done,'operation':'gaia_solve'}}
        result=self.worker.dispatch('environment_busy',{'_science_role':'view'})
        self.assertEqual(result['active_jobs'],1);self.assertTrue(result['installing']);self.assertIn('two',self.worker.jobs)
        self.worker.jobs.clear()

    def test_five_cds_presets_and_required_archive_sources(self):
        result=self.worker.dispatch('service_presets',{})
        self.assertEqual(len(result['cds']),5);ids={item['id'] for item in result['archives']}
        self.assertTrue({'sdss_dr17','des_dr2','des_dr2_se','aladin_hips','mast','eso','cadc','irsa','alma'}<=ids)

    def test_gaia_three_arcsec_default_keeps_independent_quality_rejection(self):
        import fitsviewer_gaia as gaia
        from astropy.wcs.utils import pixel_to_skycoord
        path,wcs=self.image();xx,yy=np.meshgrid(np.linspace(8,92,6),np.linspace(8,72,6));points=np.column_stack((xx.ravel(),yy.ravel()))
        sky=pixel_to_skycoord(points[:,0],points[:,1],wcs,origin=0).icrs
        rng=np.random.default_rng(18102026);angle=rng.uniform(0,2*np.pi,len(points))
        def table(amplitude):return Table({'source_id':np.arange(len(points)),'ra':sky.ra.deg+amplitude*np.cos(angle)/(3600*np.cos(np.deg2rad(sky.dec.deg))),
                                          'dec':sky.dec.deg+amplitude*np.sin(angle)/3600,'phot_g_mean_mag':np.full(len(points),16.)})
        with patch.object(gaia,'reference_table',return_value=(table(1.5),'synthetic Gaia references',None)),patch.object(gaia,'detected_points',return_value=points):
            value=gaia.solve(self.worker,{'path':str(path)});self.assertTrue(value['accepted']);self.assertGreater(value['validation_rms_arcsec'],1.)
            self.assertGreaterEqual(value['validation_stars'],3)
            with self.assertRaisesRegex(ValueError,'rejected'):gaia.solve(self.worker,{'path':str(path),'max_residual_arcsec':1.})
        with patch.object(gaia,'reference_table',return_value=(table(4.5),'synthetic bad references',None)),patch.object(gaia,'detected_points',return_value=points):
            with self.assertRaisesRegex(ValueError,'rejected'):gaia.solve(self.worker,{'path':str(path)})

    def test_export_rotation_shares_raw_pixel_transform_and_exact_viewport(self):
        from matplotlib.figure import Figure
        from matplotlib.backends.backend_agg import FigureCanvasAgg
        from matplotlib.transforms import Affine2D
        from matplotlib.font_manager import FontProperties
        from fitsviewer_export import rotated_view,rotated_wcs,annotation
        _,wcs=self.image();data=np.ones((80,100));center=np.array([49.5,39.5]);screen_box=np.array([[20,10],[80,10],[80,70],[20,70]])
        raw=Affine2D().rotate_deg_around(*center,45).transform(screen_box)
        for world in [False,True]:
            figure=Figure();FigureCanvasAgg(figure);ax=figure.add_subplot(111,projection=wcs if world else None)
            affine=rotated_view(ax,data,{'rotation_deg':45,'viewport_polygon':raw.tolist()})
            if world:ax.reset_wcs(wcs=rotated_wcs(wcs,affine))
            pixels=affine+ax.transData
            image=ax.imshow(data,origin='lower',transform=pixels);annotation(ax,{'kind':'circle','center':[10,20],'radius':2},FontProperties(),wcs,pixels)
            np.testing.assert_allclose(ax.get_xlim(),[20,80],atol=1e-9);np.testing.assert_allclose(ax.get_ylim(),[10,70],atol=1e-9)
            self.assertIs(image.get_transform(),pixels)
            np.testing.assert_allclose(affine.transform(center),center,atol=1e-9)
            if world:
                ax.grid();displayed=affine.transform(np.array([[10.,20.],[60.,50.]]));expected=wcs.pixel_to_world_values([10.,60.],[20.,50.])
                actual=ax.wcs.pixel_to_world_values(displayed[:,0],displayed[:,1]);np.testing.assert_allclose(actual,expected,atol=1e-10)
                np.testing.assert_allclose(np.asarray(ax.wcs.world_to_pixel_values(*expected)).T,displayed,atol=1e-6)
            figure.canvas.draw();figure.clear()

    def test_moc_uses_declared_latin1_and_does_not_claim_exposures(self):
        path,_=self.image();records=[{'ID':'survey','obs_title':'Données publiques','hips_service_url':'https://example.invalid/hips','dataproduct_type':'image','hips_tile_format':'fits'}]
        with patch.object(archives,'fetch_bytes',return_value=(json.dumps(records,ensure_ascii=False).encode('latin1'),{'encoding':'ISO-8859-1'})):
            result=archives.search(self.worker,{'service_id':'aladin_hips','path':str(path)})
        self.assertIn('Données publiques',result['products'][0]['title']);self.assertEqual(result['coverage_kind'],'current_image_cutouts')
        self.assertEqual(result['products'][0]['download_kind'],'hips_cutout');self.assertIn('not an original exposure',result['products'][0]['processing'])

    def test_image_center_and_inherited_projection_ignore_cursor_position(self):
        from astropy.wcs.utils import pixel_to_skycoord
        path,wcs=self.image();self.worker.wcs({'path':str(path),'points':[[2.,3.]]})
        center=self.worker.image_center({'path':str(path)})
        self.assertEqual(center['pixel'],[49.5,39.5]);self.assertAlmostEqual(center['icrs']['ra_deg'],359.999,places=9)
        result=self.worker.dispatch('reproject',{'path':str(path),'use_image_center':True,'scale_arcsec':None,'width':50,'height':40})
        self.assertTrue(result['use_image_center']);self.assertAlmostEqual(result['scale_arcsec'],np.sqrt(1.04),places=5)
        with fits.open(result['_path']) as hdus:
            target=WCS(hdus[0].header).celestial;sky=pixel_to_skycoord(24.5,19.5,target,origin=0).icrs
        self.assertLess(sky.separation(pixel_to_skycoord(49.5,39.5,wcs,origin=0)).arcsec,1e-5)

    def test_base_dependency_job_accepts_view_role_and_preserves_undelivered_result(self):
        owner={'_viewer_session':'dependency-owner','_document_generation':1,'_science_role':'view','_role_epoch':0}
        start=self.worker.dispatch('job_start',dict(owner,operation='dependency_probe',params={'packages':[],'_science_role':'view','_document_generation':1}))
        identifier=start['job_id'];self.worker.jobs[identifier]['future'].result(timeout=15)
        busy=self.worker.dispatch('environment_busy',{})
        self.assertEqual(busy['active_jobs'],0);self.assertEqual(busy['pending_results'],1)
        self.worker.dispatch('release_pro_owner',{'_viewer_session':'other-view','_science_role':'pro'})
        self.assertEqual(self.worker.dispatch('environment_busy',{})['pending_results'],1)
        with self.assertRaisesRegex(ValueError,'different viewer'):
            self.worker.dispatch('job_status',{'job_id':identifier,'_viewer_session':'other-view'})
        reply=self.worker.dispatch('job_status',dict(owner,job_id=identifier))
        self.assertEqual(reply['state'],'completed');self.assertEqual(reply['result']['packages'],[])
        self.assertEqual(self.worker.dispatch('environment_busy',{})['pending_results'],0)
        self.assertEqual(self.worker.dispatch('job_status',dict(owner,job_id=identifier))['state'],'completed')

    def test_archive_center_and_fk5_input_are_explicitly_icrs(self):
        from astropy.coordinates import SkyCoord,FK5
        from astropy.time import Time
        ra,dec,_=archives.region({'ra_deg':130.,'dec_deg':12.,'frame':'fk5'})
        truth=SkyCoord(130,12,unit='deg',frame=FK5(equinox=Time('J2000'))).icrs
        self.assertAlmostEqual(ra,truth.ra.deg,places=10);self.assertAlmostEqual(dec,truth.dec.deg,places=10)
        path,_=self.image()
        with patch.object(archives,'fetch_bytes',return_value=(b'[]',{'encoding':'utf-8'})) as fetch:
            result=archives.search(self.worker,{'service_id':'aladin_hips','path':str(path),'use_image_center':True,'ra_deg':1.,'dec_deg':2.,'frame':'fk5'})
        self.assertAlmostEqual(result['query_region']['ra_deg'],359.999,places=9)
        self.assertTrue(fetch.call_args.kwargs['params']['stc'].startswith('Polygon '));self.assertNotIn('RA',fetch.call_args.kwargs['params'])


if __name__=='__main__':unittest.main()
