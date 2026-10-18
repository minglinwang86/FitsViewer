"""Current-field survey selection and cutout download contracts.

球面面积用独立的矩形立体角公式核对；合成输入只用于可控边界与负例。
"""
from pathlib import Path
from unittest.mock import patch
import bz2,io,json,math,sys,tempfile,threading,unittest,urllib.parse

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'web/python'))
import numpy as np
from astropy.io import fits
from astropy.wcs import WCS
from astropy.table import Table
import fitsviewer_archive_geometry as geometry
import fitsviewer_archive_services as archives
from fitsviewer_network import ServiceError
import fitsviewer_science as science
import fitsviewer_vo as vo


def rectangle(a,b):
    points=np.array([[1,-a,-b],[1,a,-b],[1,a,b],[1,-a,b]],float)
    return points/np.linalg.norm(points,axis=1)[:,None]


def rectangle_area(a,b):
    return 4*math.atan(a*b/math.sqrt(1+a*a+b*b))*(180/math.pi)**2


def stcs(points):return 'POLYGON ICRS '+' '.join(f'{v:.14g}' for pair in geometry.coordinates(points) for v in pair)


class Response(io.BytesIO):
    def __init__(self,data,mime='application/fits'):
        super().__init__(data);self.headers={'Content-Type':mime,'Content-Length':str(len(data)),'Content-Disposition':'attachment; filename="../../overwrite.fits"'}
    def geturl(self):return 'https://public.example/cutout'


class SurveyDownload(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='survey-contract-',dir=ROOT/'qa');self.path=Path(self.temp.name);self.worker=science.Worker(self.path/'cache')
    def tearDown(self):self.worker.close();self.temp.cleanup()
    def image(self,width=300,height=100,ra=359.999,dec=65.):
        wcs=WCS(naxis=2);wcs.wcs.ctype=['RA---TAN','DEC--TAN'];wcs.wcs.crval=[ra,dec];wcs.wcs.crpix=[(width+1)/2,(height+1)/2]
        angle=math.radians(31);wcs.wcs.cd=np.array([[-math.cos(angle),math.sin(angle)],[math.sin(angle),math.cos(angle)]])/3600
        path=self.path/'current.fits';fits.PrimaryHDU(np.ones((height,width),dtype='f4'),header=wcs.to_header()).writeto(path,overwrite=True)
        return path,wcs
    def field(self):
        shape=rectangle(.005,.003)
        return {'field_boundary_icrs':geometry.coordinates(shape),'field_area_sqdeg':rectangle_area(.005,.003)}
    def product(self,ratio,name):
        a=.005;t=math.tan(self.field()['field_area_sqdeg']*ratio/(180/math.pi)**2/4)
        b=math.sqrt(t*t*(1+a*a)/(a*a-t*t))
        return {'id':name,'footprint':stcs(rectangle(a,b))}

    def test_spherical_area_and_intersection_match_independent_solid_angle(self):
        shape=rectangle(.3,.2)
        self.assertAlmostEqual(geometry.area(shape),rectangle_area(.3,.2),places=10)
        octant=geometry.vectors([[0,0],[90,0],[0,90]])
        self.assertAlmostEqual(geometry.area(octant),math.pi/2*(180/math.pi)**2,places=10)
        half=np.array([[1,0,-.2],[1,.3,-.2],[1,.3,.2],[1,0,.2]],float);half/=np.linalg.norm(half,axis=1)[:,None]
        self.assertAlmostEqual(geometry.area(geometry.intersection(shape,half)),rectangle_area(.3,.2)/2,places=9)
        remote=geometry.vectors([[179,-1],[181,-1],[181,1],[179,1]])
        self.assertEqual(geometry.area(geometry.intersection(shape,remote)),0.)

    def test_area_ratio_closed_boundaries_and_oversized_exclusion(self):
        products=[self.product(.4,'forty'),self.product(1.5,'one-fifty'),self.product(1.50001,'too-large'),self.product(.39999,'partial')]
        selected,info=geometry.select_products(products,self.field())
        self.assertEqual({row['id'] for row in selected},{'forty','one-fifty'});self.assertEqual(info['over_150_percent'],1)
        self.assertFalse(info['partial_fallback']);self.assertAlmostEqual(products[0]['area_ratio'],.4,places=9)
        self.assertAlmostEqual(products[1]['area_ratio'],1.5,places=9)

    def test_simple_concave_wcs_area_and_overlap_match_independent_quadrature(self):
        a,b=.005,.003;points=np.array([[1,-a,-b],[1,a,-b],[1,a,b],[1,0,b/2],[1,-a,b]],float);points/=np.linalg.norm(points,axis=1)[:,None]
        nodes,weights=np.polynomial.legendre.leggauss(48);x=(nodes+1)*a/2;top=b/2+x*b/(2*a)
        y=-b+(nodes[:,None]+1)*(top+b)/2
        oracle=float(2*np.sum(weights*np.sum(weights[:,None]*(1+x[None,:]**2+y*y)**-1.5,axis=0)*(top+b)/2)*a/2)*(180/math.pi)**2
        self.assertFalse(geometry.convex(points));self.assertIsNotNone(geometry.components(points));self.assertAlmostEqual(geometry.area(points),oracle,places=11)
        self.assertAlmostEqual(geometry.overlap_area(rectangle(a,b),points),oracle,places=11)
        repeated=np.vstack((rectangle(a,b),rectangle(a,b)));self.assertIsNone(geometry.components(repeated))
        crossed=points[[0,2,1,3,4]];self.assertIsNone(geometry.components(crossed))

    def test_sia1_complete_wcs_uses_standard_cd_order_and_missing_orientation_is_unknown(self):
        table=Table({'url':['https://public.example/frame.fits'],'axes':[2],'dims':[[300,100]],'frame':['ICRS'],'projection':['TAN'],
                     'refpix':[[150.5,50.5]],'refval':[[359.999,65.]],'matrix':[[-.0002,.0001,.00008,.0002]]})
        for name,ucd in [('url','VOX:Image_AccessReference'),('axes','VOX:Image_Naxes'),('dims','VOX:Image_Naxis'),('frame','VOX:STC_CoordRefFrame'),
                         ('projection','VOX:WCS_CoordProjection'),('refpix','VOX:WCS_CoordRefPixel'),('refval','VOX:WCS_CoordRefValue'),('matrix','VOX:WCS_CDMatrix')]:table[name].meta['ucd']=ucd
        product=archives.product_rows(table,archives.ARCHIVE_PRESETS[1])[0]
        self.assertIn('provider-declared',product['footprint_source']);self.assertFalse(product['footprint_header_verified'])
        self.assertEqual(product['footprint_wcs']['CD1_2'],.0001);self.assertEqual(product['footprint_wcs']['CD2_1'],.00008)
        a,b=150,50;cd=np.array([[-.0002,.0001],[.00008,.0002]])*math.pi/180
        nodes,weights=np.polynomial.legendre.leggauss(32);xx,yy=np.meshgrid(nodes*a,nodes*b);plane=cd@np.vstack((xx.ravel(),yy.ravel()))
        oracle=float(np.sum(np.outer(weights,weights).ravel()*abs(np.linalg.det(cd))*(1+(plane**2).sum(axis=0))**-1.5)*a*b)*(180/math.pi)**2
        self.assertAlmostEqual(geometry.footprint(product['footprint'])[1],oracle,places=11)
        table.remove_column('matrix');self.assertIsNone(archives.product_rows(table,archives.ARCHIVE_PRESETS[1])[0]['footprint'])

    def test_sdss_bounded_header_uses_actual_wcs_and_cancels_without_writing_files(self):
        path,wcs=self.image();url='https://dr17.sdss.org/sas/dr17/eboss/photoObj/frames/301/1/1/frame-r-000001-1-0001.fits.bz2'
        class SDSSResponse(Response):
            def geturl(self):return url
        products=[{'id':'frame','access_url':url,'footprint':None}]
        with patch.object(vo.urllib.request,'urlopen',return_value=SDSSResponse(bz2.compress(path.read_bytes()))):
            info=archives.sdss_header_footprints(self.worker,products,{'timeout':15})
        self.assertEqual(info['verified'],1);self.assertTrue(products[0]['footprint_header_verified']);self.assertEqual(products[0]['footprint_dimensions'],[300,100])
        parsed=geometry.footprint(products[0]['footprint']);self.assertAlmostEqual(parsed[1],geometry.image_field(self.worker,{'path':str(path)})['field_area_sqdeg'],places=9)
        self.assertEqual(list(self.worker.cache.iterdir()),[])
        html=[{'access_url':url}]
        with patch.object(vo.urllib.request,'urlopen',return_value=SDSSResponse(b'<html>login</html>')):
            info=archives.sdss_header_footprints(self.worker,html,{})
        self.assertEqual(info['verified'],0);self.assertIn('not a primary FITS',html[0]['footprint_error'])
        cancel=threading.Event();cancel.set()
        with patch.object(vo.urllib.request,'urlopen',return_value=SDSSResponse(path.read_bytes())):
            with self.assertRaises(InterruptedError):archives.sdss_header_footprints(self.worker,[{'access_url':url}],{'_cancel':cancel})
        untrusted=[{'access_url':'https://untrusted.example/frame.fits'}]
        with patch.object(vo.urllib.request,'urlopen') as request:archives.sdss_header_footprints(self.worker,untrusted,{});request.assert_not_called()

    def test_partial_only_when_bounded_response_has_no_preferred_and_unknown_is_diagnostic(self):
        partial=self.product(.2,'partial');unknown={'id':'unknown','footprint':'UNION ICRS ( CIRCLE 1 2 3 )'}
        outside=self.product(.8,'outside');outside['footprint']='CIRCLE ICRS 180 0 .1'
        selected,info=geometry.select_products([partial,unknown,outside],self.field())
        self.assertEqual([row['id'] for row in selected],['partial']);self.assertTrue(info['partial_fallback']);self.assertEqual(info['unknown_geometry'],1)
        self.assertEqual(info['outside'],1);self.assertIn('bounded',info['selection_scope'])
        selected,info=geometry.select_products([partial],{});self.assertEqual(selected,[]);self.assertEqual(info['unknown_geometry'],1)

    def test_invalid_and_polar_footprints_do_not_use_planar_ra_area(self):
        for value in ('CIRCLE ICRS 0 91 1','POLYGON ICRS 0 0 0 0 0 0','CIRCLE ICRS 0 0 -1','garbage',None):self.assertIsNone(geometry.footprint(value))
        pole=geometry.footprint('POLYGON ICRS 0 89 90 89 180 89 270 89')
        self.assertIsNotNone(pole);self.assertTrue(0<pole[1]<5.)
        circle=geometry.footprint('CIRCLE ICRS 359.99 65 .02')
        self.assertAlmostEqual(circle[1],2*math.pi*(1-math.cos(math.radians(.02)))*(180/math.pi)**2,places=11)
        tiny=geometry.footprint('CIRCLE ICRS 0 0 .000001')
        self.assertAlmostEqual(tiny[1]/(math.pi*1e-12),1.,places=14)

    def test_wcs_field_cutout_preserves_rotated_nonsquare_area_and_override(self):
        path,wcs=self.image();field=geometry.image_field(self.worker,{'path':str(path)})
        self.assertAlmostEqual(field['ra_deg'],359.999,places=9);self.assertAlmostEqual(field['dec_deg'],65.,places=9)
        self.assertAlmostEqual(field['cutout_area_ratio'],1.,places=7)
        definition=field['cutout_wcs'];self.assertAlmostEqual(definition['NAXIS1']/definition['NAXIS2'],3.,delta=.01)
        # The old enclosing-cone square would exceed the user's 150% ceiling.
        self.assertGreater((2*field['radius_deg'])**2/field['field_area_sqdeg'],1.5)
        header=wcs.to_header();header['CRVAL1']=12.
        changed=geometry.image_field(self.worker,{'path':str(path),'wcs_override':header.tostring(sep='\n')})
        self.assertAlmostEqual(changed['ra_deg'],12.,places=9)

    def test_missing_wcs_or_missing_current_file_fails_before_network(self):
        path=self.path/'no-wcs.fits';fits.PrimaryHDU(np.ones((10,10))).writeto(path)
        for params in ({'service_id':'aladin_hips'},{'service_id':'aladin_hips','path':str(path)}):
            with patch.object(archives,'fetch_bytes') as request:
                with self.assertRaisesRegex(ValueError,'WCS|celestial'):archives.search(self.worker,params)
                request.assert_not_called()

    def test_moc_discovery_uses_field_polygon_and_excludes_cube_color_and_jpeg(self):
        path,_=self.image();base={'dataproduct_type':'image','hips_tile_format':'fits','hips_service_url':'https://public.example/hips'}
        records=[dict(base,ID='CDS/P/LABH',hips_cube_depth='100'),dict(base,ID='CDS/P/RGB',dataproduct_subtype='color'),
                 dict(base,ID='CDS/P/Preview',hips_tile_format='jpeg'),dict(base,ID='CDS/P/Other'),dict(base,ID='CDS/P/DSS2/red')]
        with patch.object(archives,'fetch_bytes',return_value=(json.dumps(records).encode(),{'encoding':'utf-8'})) as request:
            result=archives.search(self.worker,{'service_id':'aladin_hips','path':str(path),'max_rows':1})
        self.assertEqual(result['products'][0]['hips_id'],'CDS/P/DSS2/red');self.assertTrue(result['truncated'])
        self.assertEqual(result['products'][0]['coverage'],'current_image_cutout');self.assertEqual(sum(result['diagnostics']['excluded_coverage_records'].values()),3)
        self.assertIn('stc',request.call_args.kwargs['params']);self.assertEqual(request.call_args.kwargs['params']['intersect'],'overlaps')

    def test_cutout_exact_provider_verification_and_manual_area_guard(self):
        path,_=self.image();record={'ID':'CDS/P/DSS2/red','dataproduct_type':'image','hips_tile_format':'fits'}
        params={'path':str(path),'hips_id':record['ID'],'width':128,'height':64}
        with patch.object(archives,'fetch_bytes',return_value=(json.dumps([record]).encode(),{})) as request:
            url,metadata=archives.hips_cutout(self.worker,params)
        self.assertAlmostEqual(metadata['area_ratio'],1.,places=7);self.assertTrue(metadata['derived']);self.assertFalse(metadata['original_exposure'])
        self.assertEqual(request.call_args.kwargs['params']['ID'],record['ID']);self.assertIn('stc',request.call_args.kwargs['params'])
        self.assertIn('/hips-image-services/hips2fits?',url);self.assertEqual(urllib.parse.parse_qs(urllib.parse.urlsplit(url).query)['format'],['fits'])
        for bad in (dict(params,hips_id='*DSS*'),dict(params,width=True),dict(params,use_current_field=False,ra_deg=0,dec_deg=0,fov_deg=1.)):
            with patch.object(archives,'fetch_bytes') as request:
                with self.assertRaises(ValueError):archives.hips_cutout(self.worker,bad)
                request.assert_not_called()
        for row in (dict(record,ID='different'),dict(record,hips_cube_depth=8),dict(record,hips_tile_format='jpeg')):
            with patch.object(archives,'fetch_bytes',return_value=(json.dumps([row]).encode(),{})):
                with self.assertRaises(ValueError):archives.hips_cutout(self.worker,params)

    def test_download_validates_actual_wcs_finite_data_and_keeps_remote_filename_untrusted(self):
        path,_=self.image();field=geometry.image_field(self.worker,{'path':str(path)});definition=field['cutout_wcs'];width,height=definition['NAXIS1'],definition['NAXIS2']
        metadata={'cutout':{'width':width,'height':height,'wcs':json.dumps(definition)},'derived':True}
        def payload(header,data):
            stream=io.BytesIO();fits.PrimaryHDU(data,header=WCS(header).to_header()).writeto(stream);return stream.getvalue()
        for bad in ('wrong-wcs','nan','html'):
            header=dict(definition);data=np.ones((height,width),dtype='f4')
            if bad=='wrong-wcs':header['CRVAL1']+=1.
            if bad=='nan':data[:]=np.nan
            body=b'<html>error</html>' if bad=='html' else payload(header,data)
            with patch.object(archives,'hips_cutout',return_value=('https://public.example/cutout',dict(metadata))),patch.object(vo.urllib.request,'urlopen',return_value=Response(body)):
                with self.assertRaises(ServiceError if bad=='html' else ValueError):vo.download(self.worker,{'hips_cutout':{},'path':str(path)})
            self.assertEqual(list(self.worker.cache.iterdir()),[])
        with patch.object(archives,'hips_cutout',return_value=('https://public.example/cutout',dict(metadata))),patch.object(vo.urllib.request,'urlopen',return_value=Response(payload(definition,np.ones((height,width),dtype='f4')))):
            result=vo.download(self.worker,{'hips_cutout':{},'path':str(path)})
        self.assertEqual(result['kind'],'fits');self.assertTrue(result['returned_wcs_verified']);self.assertTrue(Path(result['_path']).name.startswith('download-'))
        self.assertFalse((self.path/'overwrite.fits').exists())

    def test_filecap_rejects_new_download_before_network_and_midstream_cancel_leaves_no_part(self):
        with patch.object(vo.urllib.request,'urlopen') as request:
            with self.assertRaises(PermissionError):vo.download(self.worker,{'_file_capability':True,'url':'https://public.example/a.fits'})
            request.assert_not_called()
        cancel=threading.Event()
        class CancelResponse(Response):
            def read(self,size=-1):cancel.set();return super().read(size)
        with patch.object(vo.urllib.request,'urlopen',return_value=CancelResponse(b'SIMPLE  ='+b' '*10000)):
            with self.assertRaises(InterruptedError):vo.download(self.worker,{'url':'https://public.example/a.fits','_cancel':cancel})
        self.assertEqual(list(self.worker.cache.iterdir()),[])


if __name__=='__main__':unittest.main()
