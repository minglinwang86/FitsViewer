"""File-only table parsing cannot inherit external XML resources; no real network."""
from pathlib import Path
from concurrent.futures import Future
import bz2,gzip,io,json,struct,sys,tempfile,threading,unittest
from types import SimpleNamespace
from unittest.mock import patch

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1];sys.path.insert(0,str(ROOT/'web/python'))
import fitsviewer_science as science
import fitsviewer_network as network
import fitsviewer_gaia as gaia
import fitsviewer_tap as tap
import fitsviewer_archive_services as archives
import fitsviewer_vo as vo
from astropy.table import Table
from astropy.io import fits
from astropy.io.votable import from_table,writeto
import numpy as np


def external(href='http://127.0.0.1:1/ungranted',attribute='href'):
    return ('''<?xml version="1.0"?><VOTABLE version="1.3" xmlns="http://www.ivoa.net/xml/VOTable/v1.3" xmlns:xlink="http://www.w3.org/1999/xlink"><RESOURCE><TABLE><FIELD name="value" datatype="char" arraysize="*"/><DATA><BINARY><STREAM '''+attribute+'="'+href+'''"/></BINARY></DATA></TABLE></RESOURCE></VOTABLE>''').encode()


def inline_table():
    stream=io.BytesIO();writeto(from_table(Table({'ra':[1.,2.],'dec':[3.,4.]})),stream)
    return stream.getvalue()


class FileCapabilityTables(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(prefix='file-capability-table-',dir=ROOT/'qa');self.path=Path(self.temp.name)
        self.worker=science.Worker(self.path/'cache')
        self.no_network=patch('urllib.request.urlopen',side_effect=AssertionError('Real URL I/O is forbidden'));self.url=self.no_network.start()

    def tearDown(self):self.worker.close();self.no_network.stop();self.temp.cleanup()

    def write(self,body,name='stable-alias.fits'):
        path=self.path/name;path.write_bytes(body);return path

    def test_stream_href_is_rejected_before_astropy_or_url_io(self):
        for href,attribute in [('http://127.0.0.1:1/private','href'),('file:///ungranted','href'),('../other.dat','href'),('','href'),('https://public.example/data','xlink:href')]:
            with self.subTest(href=href,attribute=attribute),patch.object(Table,'read') as reader:
                with self.assertRaisesRegex(ValueError,'self-contained.*STREAM'):
                    network.read_table(self.write(external(href,attribute)),file_capability=True,format='votable')
                reader.assert_not_called();self.url.assert_not_called()

    def test_body_guard_precedes_format_dispatch_and_alias_suffix(self):
        for format_ in [None,'fits','ascii.csv','votable']:
            with self.subTest(format=format_),patch.object(Table,'read') as reader:
                with self.assertRaisesRegex(ValueError,'STREAM'):
                    network.read_table(self.write(external()),file_capability=True,**({'format':format_} if format_ else {}))
                reader.assert_not_called()

    def test_dtd_custom_entities_and_encoded_external_stream_are_rejected(self):
        documents=[b'<!DOCTYPE VOTABLE SYSTEM "file:///ungranted"><VOTABLE/>',
                   b'<!DOCTYPE VOTABLE [<!ENTITY leak SYSTEM "http://127.0.0.1:1/private">]><VOTABLE>&leak;</VOTABLE>',
                   b'<!DOCTYPE VOTABLE [<!ENTITY % leak SYSTEM "../private.dtd">%leak;]><VOTABLE/>',
                   external().decode().replace('version="1.0"','version="1.0" encoding="utf-16"').encode('utf-16')]
        for body in documents:
            with self.subTest(body=body[:80]),patch('astropy.io.votable.parse') as parser:
                with self.assertRaisesRegex(ValueError,'self-contained'):
                    network.parse_votable(body,file_capability=True)
                parser.assert_not_called();self.url.assert_not_called()

    def test_compression_cannot_hide_external_stream(self):
        for compressor in [gzip.compress,bz2.compress,lambda body:gzip.compress(gzip.compress(body))]:
            with self.subTest(compressor=compressor):
                with self.assertRaisesRegex(ValueError,'STREAM'):
                    network.read_table(self.write(compressor(external())),file_capability=True)
                with self.assertRaisesRegex(ValueError,'STREAM'):
                    network.parse_votable(compressor(external()),file_capability=True)

    def test_decoded_table_size_and_compression_layers_are_bounded(self):
        with patch.object(network,'MAX_TABLE',1024):
            with self.assertRaisesRegex(ValueError,'Decoded.*64 MiB'):
                network.read_table(self.write(gzip.compress(b' '*2048)),file_capability=True)
            with self.assertRaisesRegex(ValueError,'64 MiB'):
                network.parse_votable(b' '*2048,file_capability=True)
        body=inline_table()
        for _ in range(4):body=gzip.compress(body)
        with self.assertRaisesRegex(ValueError,'compression layers'):network.parse_votable(body,file_capability=True)
        with self.assertRaisesRegex(ValueError,'ZIP'):network.parse_votable(b'PK\x03\x04',file_capability=True)

    def test_self_contained_votable_preserves_link_and_datalink_urls(self):
        body=inline_table().replace(b'<TABLE>',b'<LINK href="https://public.example/metadata?a=1&amp;b=2"/><TABLE>')
        table,meta=network.parse_votable(body,file_capability=True)
        self.assertEqual(list(table['ra']),[1.,2.]);self.assertEqual(meta['query_status'],'OK')
        self.assertEqual(len(network.read_table(self.write(gzip.compress(body)),file_capability=True)),2)
        self.url.assert_not_called()

    def test_normal_escapes_and_cdata_are_not_false_positives(self):
        body=inline_table().replace(b'<TABLE>',b'<TABLE><DESCRIPTION><![CDATA[Literal <!DOCTYPE text>]]> &amp; &#65;</DESCRIPTION>')
        table,_=network.parse_votable(body,file_capability=True)
        self.assertEqual(len(table),2);self.url.assert_not_called()

    def test_csv_and_fits_tables_work_without_filename_inference(self):
        csv=network.read_table(self.write(b'ra,dec\n1,2\n3,4\n','stars.csv'),file_capability=True)
        self.assertEqual(list(csv.colnames),['ra','dec']);self.assertEqual(list(csv['ra']),[1,3])
        alias=network.read_table(self.write(b'ra,dec\n1,2\n3,4\n'),file_capability=True,format='ascii.csv')
        self.assertEqual(list(alias['dec']),[2,4])
        plain=network.read_table(self.write(b'ra dec\n1 2\n3 4\n','stars.dat'),file_capability=True)
        self.assertEqual(list(plain['ra']),[1,3])
        path=self.path/'table.fits';Table({'ra':[1.,2.]}).write(path,format='fits')
        self.assertEqual(list(network.read_table(path,file_capability=True)['ra']),[1.,2.])
        self.assertEqual(list(network.read_table(self.write(gzip.compress(path.read_bytes()),'table-copy.fits.gz'),file_capability=True)['ra']),[1.,2.])

    def test_table_bound_does_not_change_fits_image_reads(self):
        path=self.path/'image.fits';fits.PrimaryHDU(np.ones((3,3))).writeto(path)
        self.worker.scope.file_capability=True
        with patch.object(network,'MAX_TABLE',64):
            _,_,_,array=self.worker.image({'path':str(path)},data=True)
        self.assertEqual(array.shape,(3,3))

    def test_restricted_formats_fail_before_dynamic_reader_dispatch(self):
        for format_ in ['hdf5','parquet','votable.parquet','pandas.html']:
            with self.subTest(format=format_),patch.object(Table,'read') as reader:
                with self.assertRaisesRegex(ValueError,'FITS, ASCII/CSV and VOTable'):
                    network.read_table(self.write(inline_table()),file_capability=True,format=format_)
                reader.assert_not_called()

    def test_parser_consumes_the_prechecked_snapshot(self):
        path=self.write(inline_table());original=Table.read
        def change_source(source,**kwargs):
            path.write_bytes(external())
            return original(source,**kwargs)
        with patch.object(Table,'read',side_effect=change_source):table=network.read_table(path,file_capability=True)
        self.assertEqual(list(table['ra']),[1.,2.]);self.url.assert_not_called()

    def test_trusted_reader_and_parser_keep_explicit_external_stream_behavior(self):
        value=b'controlled trusted fixture';wire=struct.pack('>I',len(value))+value
        with patch('urllib.request.urlopen',side_effect=lambda *a,**k:io.BytesIO(wire)) as fetch:
            table=network.read_table(self.write(external()),format='votable')
            parsed,_=network.parse_votable(external())
        self.assertEqual(str(table['value'][0]),value.decode());self.assertEqual(str(parsed['value'][0]),value.decode());self.assertEqual(fetch.call_count,2)

    def test_trusted_cache_cannot_bypass_file_capability_guard(self):
        value=b'controlled trusted fixture';wire=struct.pack('>I',len(value))+value;path=self.write(external())
        params={'catalogue':str(path),'format':'votable'}
        with patch('urllib.request.urlopen',return_value=io.BytesIO(wire)):
            self.assertEqual(self.worker.dispatch('catalogue',params)['rows'],1)
        with self.assertRaisesRegex(ValueError,'STREAM'):self.worker.dispatch('catalogue',dict(params,_file_capability=True))
        self.url.assert_not_called()

    def test_nested_catalogue_cannot_disable_outer_scope(self):
        with self.assertRaisesRegex(ValueError,'STREAM'):
            self.worker.dispatch('xmatch',{'_file_capability':True,'first':{'catalogue':str(self.write(external())),'format':'votable','_file_capability':False},
                                            'second':{'ra_deg':[1.],'dec_deg':[2.]}})
        self.url.assert_not_called()

    def test_outer_job_flag_wins_and_executor_context_keeps_it(self):
        future=Future();params={'_file_capability':True,'operation':'export','params':{'_file_capability':False,'catalogue':str(self.write(external())),'format':'votable'}}
        with patch.object(self.worker.executor,'submit',return_value=future) as submit:
            self.worker.dispatch('job_start',params)
        options=submit.call_args.args[2];self.assertIs(options['_file_capability'],True)
        with patch('fitsviewer_export.request',side_effect=lambda worker,op,p:worker.catalogue(dict(p,_file_capability=False))):
            with self.assertRaisesRegex(ValueError,'STREAM'):self.worker.run_job('export',options,threading.Event())
        self.url.assert_not_called()

    def test_gaia_local_and_response_tables_share_the_guard(self):
        path=self.write(external())
        with self.assertRaisesRegex(ValueError,'STREAM'):gaia.reference_table(self.worker,None,None,{'catalogue':str(path),'_file_capability':True})
        center=SimpleNamespace(ra=SimpleNamespace(deg=1.),dec=SimpleNamespace(deg=2.))
        with patch('fitsviewer_tap.query',return_value={'_path':str(path)}) as query:
            with self.assertRaisesRegex(ValueError,'STREAM'):gaia.reference_table(self.worker,center,.01,{'_file_capability':True})
        self.assertIs(query.call_args.args[1]['_file_capability'],True);self.url.assert_not_called()

    def test_public_tap_primary_request_remains_but_external_result_is_rejected(self):
        self.worker.scope.file_capability=True
        with patch('fitsviewer_network.fetch_bytes',return_value=(external(),{})) as primary:
            with self.assertRaisesRegex(ValueError,'STREAM'):
                tap.query(self.worker,{'protocol':'tap','service':'https://public.example/tap','adql':'SELECT TOP 1 * FROM public_table'})
        self.assertEqual(primary.call_count,1);self.url.assert_not_called()

    def test_archive_cached_votable_is_guarded_and_flag_reaches_primary_query(self):
        path=self.write(external())
        with patch('fitsviewer_archive_services.query',return_value={'_path':str(path),'status':'ok'}) as query:
            with self.assertRaisesRegex(ValueError,'STREAM'):
                archives.search(self.worker,{'_file_capability':True,'service_id':'sdss_dr17','use_image_center':False,'ra_deg':1.,'dec_deg':2.})
        self.assertIs(query.call_args.args[1]['_file_capability'],True);self.url.assert_not_called()

    def test_cds_local_votable_is_rejected_before_upload(self):
        path=self.write(external())
        with patch('astroquery.xmatch.XMatchClass.query') as upload:
            with self.assertRaisesRegex(ValueError,'STREAM'):
                vo.xmatch(self.worker,{'_file_capability':True,'catalogue':str(path),'ra_column':'value','dec_column':'value','remote_catalogue':'vizier:I/355/gaiadr3'})
        upload.assert_not_called();self.url.assert_not_called()

    def test_file_capability_cannot_delegate_parsing_to_async_tap(self):
        with self.assertRaisesRegex(ValueError,'self-contained TAP'):
            tap.query(self.worker,{'_file_capability':True,'protocol':'tap','service':'https://public.example/tap','adql':'SELECT TOP 1 * FROM public_table','async':True})
        self.url.assert_not_called()


if __name__=='__main__':unittest.main()
