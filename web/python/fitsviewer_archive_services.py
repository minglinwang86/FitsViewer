"""Nearby survey coverage and observation products, with explicit download semantics.

巡天覆盖不等于独立曝光；只查询元数据，DataLink 需先解析并选择具体产品。
"""
import io,json,math,re,urllib.parse,uuid
from fitsviewer_science import require,clean,MAX_TABLE
from fitsviewer_tap import http_url,query
from fitsviewer_network import fetch_bytes,parse_votable,ServiceError,read_table,file_capability_request
from fitsviewer_catalogue_services import CDS_PRESETS,CATALOGUE_PRESETS

ARCHIVE_PRESETS=[
    {'id':'aladin_hips','label':'Aladin / CDS current-field cutouts','protocol':'moc','urls':['https://alasky.cds.unistra.fr/MocServer/query','https://alaskybis.cds.unistra.fr/MocServer/query'],'product_scope':'resampled 2D FITS cutouts of the current image footprint; not individual exposures'},
    {'id':'sdss_dr17','label':'SDSS DR17 images','protocol':'sia','urls':['https://skyserver.sdss.org/dr17/SkyServerWS/SIAP/getSIAP'],'product_scope':'ugriz calibrated survey frames; compressed FITS'},
    {'id':'des_dr2','label':'DES DR2 coadds','protocol':'sia','urls':['https://datalab.noirlab.edu/sia/des_dr2'],'product_scope':'DES stacked/coadded images and associated products'},
    {'id':'des_dr2_se','label':'DES DR2 single exposures','protocol':'sia','urls':['https://datalab.noirlab.edu/sia/des_dr2_se'],'product_scope':'DES single epoch images'},
    {'id':'legacy_dr9','label':'Legacy Surveys DR9','protocol':'sia','urls':['https://datalab.noirlab.edu/sia/ls_dr9'],'product_scope':'DECaLS/BASS/MzLS survey products'},
    {'id':'noirlab_calibrated','label':'NOIRLab calibrated images','protocol':'sia','urls':['https://datalab.noirlab.edu/sia/calibrated_all'],'product_scope':'calibrated archive images; inspect processing and product type'},
    {'id':'mast','label':'MAST (HST/JWST and other missions)','protocol':'tap','urls':['https://mast.stsci.edu/api/v0/invoke','https://mast.stsci.edu/vo-tap/api/v0.1/caom'],'table':'ivoa.obscore','center_coverage':True,'collections':['HST','JWST','GALEX','TESS','Kepler','K2','SWIFT','IUE','FUSE'],'product_scope':'archive observation footprints; request an observation product list before choosing a file'},
    {'id':'eso','label':'ESO archive','protocol':'tap','urls':['https://archive.eso.org/tap_obs'],'table':'ivoa.obscore','product_scope':'ESO observation products; DataLink and access rights apply'},
    {'id':'cadc','label':'CADC multi-collection archive','protocol':'tap','urls':['https://ws.cadc-ccda.hia-iha.nrc-cnrc.gc.ca/argus'],'table':'ivoa.ObsCore','unsupported_columns':['obs_title'],'product_scope':'multi-facility observation products; inspect collection and DataLink'},
    {'id':'irsa','label':'IRSA (WISE/2MASS/Spitzer and other collections)','protocol':'sia2','urls':['https://irsa.ipac.caltech.edu/SIA'],'default_collection':'wise_allwise',
     'collections':['wise_allwise','twomass_allsky','neowiser','euclid_DpdMerBksMosaic','euclid_DpdNirCalibratedFrame','euclid_DpdVisCalibratedQuadFrame'],'product_scope':'image inventories including science, weight and auxiliary products; inspect footprint checks'},
    {'id':'alma','label':'ALMA archive','protocol':'tap','urls':['https://almascience.eso.org/tap','https://almascience.nrao.edu/tap','https://almascience.nao.ac.jp/tap'],'table':'ivoa.ObsCore','product_scope':'millimeter/submillimeter observations, images and cubes; resolve product links'},
    {'id':'astron','label':'ASTRON (LoTSS/LoLSS and other collections)','protocol':'tap','urls':['https://vo.astron.nl/tap'],'table':'ivoa.obscore','product_scope':'radio survey images; mosaics can be very large'},
]


def presets(_worker,_params):
    return {'cds':CDS_PRESETS,'catalogues':CATALOGUE_PRESETS,'archives':ARCHIVE_PRESETS,
            'scientific_note':'Entries are metadata search interfaces. Empty results, access rights, coverage and per-product downloads remain explicit; no blanket availability guarantee is made.'}


def region(params):
    try:ra,dec,radius=float(params['ra_deg']),float(params['dec_deg']),float(params.get('radius_deg',.01))
    except (KeyError,TypeError,ValueError):raise ValueError('Provide RA, Dec and radius in decimal degrees') from None
    if not all(math.isfinite(value) for value in [ra,dec,radius]) or not -90<=dec<=90 or not .000001<=radius<=1.5:raise ValueError('Provide finite RA/Dec and radius between 0.000001 and 1.5 degrees')
    frame=params.get('frame','icrs')
    if frame=='fk5':
        from astropy.coordinates import SkyCoord,FK5
        from astropy.time import Time
        sky=SkyCoord(ra,dec,unit='deg',frame=FK5(equinox=Time('J2000'))).icrs;ra,dec=float(sky.ra.deg),float(sky.dec.deg)
    elif frame!='icrs':raise ValueError('Archive coordinates must be ICRS or FK5 J2000')
    return ra%360,dec,radius


def scalar(value):
    import numpy as np
    if np.ma.is_masked(value):return None
    if isinstance(value,bytes):return value.decode('utf-8','replace')
    if hasattr(value,'item'):value=value.item()
    return clean(value)


def footprint_check(footprint,ra,dec,radius):
    """Check simple STC-S footprints on the sphere, including RA wrap/polar fields.

    只验证明确的球面点/圆/多边形；复合STC-S或缺失元数据保留未验证状态。
    """
    import numpy as np
    from astropy.coordinates import SkyCoord,FK5
    from astropy.time import Time
    if not isinstance(footprint,str) or not footprint.strip() or len(footprint)>128000:return None
    tokens=footprint.split();kind=tokens.pop(0).upper();frame='icrs'
    if tokens and tokens[0].upper() in ['ICRS','FK5','J2000']:frame=tokens.pop(0).lower()
    try:values=np.array([float(value) for value in tokens],dtype=float)
    except ValueError:return None
    if not np.isfinite(values).all():return None
    skyframe=FK5(equinox=Time('J2000')) if frame in ['fk5','j2000'] else 'icrs'
    query=SkyCoord(ra,dec,unit='deg',frame='icrs')
    if kind in ['CIRCLE','POSITION','POINT']:
        if len(values)!=(3 if kind=='CIRCLE' else 2) or abs(values[1])>90:return None
        size=values[2] if kind=='CIRCLE' else 0.
        if not 0<=size<=180:return None
        return bool(query.separation(SkyCoord(values[0],values[1],unit='deg',frame=skyframe)).deg<=radius+size+1e-9)
    if kind!='POLYGON' or not 6<=len(values)<=8192 or len(values)%2 or np.any(np.abs(values[1::2])>90):return None
    vertices=SkyCoord(values[0::2],values[1::2],unit='deg',frame=skyframe).icrs.cartesian.xyz.value.T
    center=query.cartesian.xyz.value;mean=vertices.sum(axis=0);norm=np.linalg.norm(mean)
    if norm<1e-10:return None
    mean/=norm
    if np.any(vertices@mean<=0):return None  # Cannot choose a unique small interior.
    dot=np.clip(vertices@center,-1,1);minimum=float(np.arccos(dot).min())
    if minimum<=math.radians(radius)+1e-10:return True
    tangent=vertices-dot[:,None]*center;length=np.linalg.norm(tangent,axis=1)
    if np.any(length<1e-12):return bool(np.any(dot>0))
    tangent/=length[:,None];following=np.roll(tangent,-1,axis=0)
    winding=np.arctan2(np.cross(tangent,following)@center,np.einsum('ij,ij->i',tangent,following)).sum()
    if center@mean>0 and abs(winding)>math.pi:return True
    for first,second in zip(vertices,np.roll(vertices,-1,axis=0)):
        normal=np.cross(first,second);length=np.linalg.norm(normal)
        if length<1e-12:continue
        normal/=length;foot=center-(center@normal)*normal;length=np.linalg.norm(foot)
        if length<1e-12:continue
        foot/=length
        edge=math.acos(float(np.clip(first@second,-1,1)))
        segments=math.acos(float(np.clip(first@foot,-1,1)))+math.acos(float(np.clip(second@foot,-1,1)))
        if abs(segments-edge)<1e-7:minimum=min(minimum,math.acos(float(np.clip(center@foot,-1,1))))
    return bool(minimum<=math.radians(radius)+1e-9)


def check_products(products,ra,dec,radius):
    counts={'overlaps_query':0,'outside_query':0,'unverified':0}
    for product in products:
        result=footprint_check(product.get('footprint'),ra,dec,radius)
        state='unverified' if result is None else 'overlaps_query' if result else 'outside_query'
        product.update(coverage_check=state,spatially_consistent=result);counts[state]+=1
    return counts


def product_rows(table,preset):
    """Use FIELD NAMEs/UCDs and preserve provenance instead of guessing a FITS URL."""
    names={name.lower():name for name in table.colnames}
    def cell(row,*aliases):
        for alias in aliases:
            name=names.get(alias.lower())
            if name is not None:
                value=scalar(row[name])
                if value is not None and str(value).strip():return value
        return None
    # SIA v1 providers use different names with standard UCDs.
    for name in table.colnames:
        ucd=str(table[name].meta.get('ucd','')).lower()
        if 'vox:image_accessreference' in ucd:names.setdefault('access_url',name)
        if 'vox:image_format' in ucd:names.setdefault('access_format',name)
        if 'vox:image_title' in ucd:names.setdefault('title',name)
        if 'vox:image_filesize' in ucd:names.setdefault('file_size',name)
    if not set(names)&{'access_url','accessreference','access_reference','acref','url','image_url','dataurl','datauri','obs_publisher_did','obs_id','productfilename','image_id','title','dataset_id'}:
        raise ServiceError('Archive result lacks observation/product identity or access columns','protocol')
    products=[];seen=set()
    for index,row in enumerate(table):
        url=cell(row,'access_url','accessreference','access_reference','acref','url','image_url','dataURL','dataURI')
        mime=str(cell(row,'access_format','format','image_format') or '')
        if url and str(url).startswith('mast:'):url='https://mast.stsci.edu/api/v0.1/Download/file?'+urllib.parse.urlencode({'uri':str(url)})
        if url:
            try:url=http_url(str(url))
            except (ValueError,TypeError):url=None
        urlpath=urllib.parse.urlsplit(url).path.lower() if url else ''
        datalink='datalink' in mime.lower() or '/datalink/' in urlpath or urlpath.endswith('/datalink')
        kind='datalink' if datalink else 'fits' if ('fits' in mime.lower() or (url and any(part in str(url).lower() for part in ['.fits','.fit','.fts','.fz']))) else 'preview' if mime.lower().startswith('image/') else 'other'
        estimate=cell(row,'access_estsize');size_unit='kbyte (1000 bytes)' if estimate is not None else None
        multiplier=1000 if estimate is not None else None
        if estimate is None:
            for alias in ['file_size','filesize','content_length','size']:
                estimate=cell(row,alias)
                if estimate is not None:
                    unit=str(table[names[alias]].unit or '').lower()
                    # MAST Product.size and DataLink content_length are defined
                    # in bytes. Other SIA fields need an explicit unit.
                    if alias in ['content_length','size'] and not unit:unit='byte'
                    multiplier={'byte':1,'bytes':1,'b':1,'kbyte':1000,'kb':1000,'kib':1024,'mbyte':1000000,'mb':1000000,'mib':1048576}.get(unit)
                    size_unit=unit or 'unspecified';break
        try:estimate=float(estimate)*multiplier if estimate is not None and multiplier is not None else None
        except (TypeError,ValueError):estimate=None
        if estimate is not None and (not math.isfinite(estimate) or estimate<0):estimate=None
        identifier=cell(row,'obs_publisher_did','productFilename','obs_id','image_id','title','dataset_id')
        key=(url,str(identifier),str(cell(row,'prodtype','product_type','dataproduct_subtype')))
        if key in seen:continue
        seen.add(key)
        products.append({'row':index,'source':preset['label'],'service_id':preset['id'],'collection':cell(row,'obs_collection','collection','survey'),
                         'facility':cell(row,'facility_name','facility','telescope'),'instrument':cell(row,'instrument_name','instrument'),
                         'id':identifier,'title':cell(row,'obs_title','title','image_title'),'product_type':cell(row,'dataproduct_type','prodtype','product_type') or ('image' if preset['protocol'] in ['sia','sia2'] else None),
                         'processing':cell(row,'proctype','dataproduct_subtype','calib_level','processing_type','productSubGroupDescription'),
                         'ra_deg':cell(row,'s_ra','ra','ra_deg'),'dec_deg':cell(row,'s_dec','dec','dec_deg'),
                         'footprint':cell(row,'s_region','footprint'),'access_url':url,'access_format':mime,
                         'estimated_bytes':int(round(estimate)) if estimate is not None else None,'original_size_unit':size_unit,'download_kind':kind,
                         'format_note':'DataLink endpoint path; provider MIME metadata is missing or malformed' if datalink and 'datalink' not in mime.lower() else None,
                         'observation_id':cell(row,'obsid','parent_obsid'),
                         'product_query_available':preset['id']=='mast' and 'productfilename' not in names,
                         'coverage':'requested_center_in_footprint' if preset.get('center_coverage') else 'service_spatial_query',
                         'semantics':str(cell(row,'product_type','dataproduct_subtype','prodtype','productType') or preset['product_scope']),
                         'time_start_mjd':cell(row,'t_min','mjd_obs'),'time_end_mjd':cell(row,'t_max'),
                         'wavelength_min_m':cell(row,'em_min'),'wavelength_max_m':cell(row,'em_max')})
        if not products[-1]['footprint'] and preset['protocol']=='sia':
            from fitsviewer_archive_geometry import sia_footprint
            declared=sia_footprint(table,row)
            if declared:products[-1].update(declared)
    return products


def mast_search(worker,params,preset,ra,dec,radius,maximum):
    """Documented CAOM JSON API, independent from the intermittently slow TAP path."""
    import numpy as np
    from astropy.table import Table,MaskedColumn
    identifier=params.get('observation_id')
    if identifier is not None and (not str(identifier).isdigit() or len(str(identifier))>24):raise ValueError('MAST observation product identifier must contain digits')
    payload={'service':'Mast.Caom.Products' if identifier is not None else 'Mast.Caom.Cone',
             'params':{'obsid':str(identifier)} if identifier is not None else {'ra':ra,'dec':dec,'radius':radius},
             'format':'json','pagesize':maximum,'page':1}
    collection=params.get('collection')
    if identifier is None and collection:
        if not isinstance(collection,str) or not 1<=len(collection)<=64:raise ValueError('Choose a mission/collection name up to 64 characters')
        payload.update(service='Mast.Caom.Filtered.Position',params={'columns':'*','filters':[{'paramName':'obs_collection','values':[collection]}],
                                                                  'position':f'{ra:.12f}, {dec:.12f}, {radius:.12f}'})
    url=preset['urls'][0];body,transport=fetch_bytes(url,data={'request':json.dumps(payload)},cancel=params.get('_cancel'),timeout=params.get('timeout',60))
    try:response=json.loads(body.decode(transport.get('encoding') or 'utf-8-sig'))
    except (ValueError,UnicodeError) as error:raise ServiceError('MAST CAOM did not return JSON','protocol',url) from error
    if not isinstance(response,dict) or response.get('status')!='COMPLETE':raise ServiceError('MAST CAOM query did not complete: '+str(response.get('msg',response.get('status')))[:1000],'service',url)
    records=response.get('data');fields=response.get('fields')
    if not isinstance(records,list) or not isinstance(fields,list) or any(not isinstance(row,dict) for row in records):raise ServiceError('MAST CAOM response has an unexpected record structure','protocol',url)
    table=Table()
    for field in fields:
        name=field['name'];kind=field.get('type');values=[row.get(name) for row in records[:maximum]];mask=[value is None for value in values]
        dtype=float if kind in ['float','double'] else 'i8' if kind in ['int','long'] else bool if kind=='boolean' else str
        fill=0 if dtype in [float,'i8',bool] else ''
        table[name]=MaskedColumn([fill if value is None else value for value in values],mask=mask,dtype=dtype)
    for name in ['em_min','em_max']:
        if name in table.colnames:table[name]*=1e-9;table[name].unit='m'
    products=product_rows(table,preset)
    for row in products:row['coverage']='observation_product_list' if identifier is not None else 'service_footprint_cone_query'
    checked=check_products(products,ra,dec,radius) if identifier is None else None
    paging=response.get('paging',{});reported=int(paging.get('rowsFiltered',len(records)))
    status='coverage_mismatch' if checked and products and checked['outside_query']==len(products) else 'ok' if products else 'empty'
    return {'status':status,'service_id':'mast','service':url,'protocol':'mast_caom','products':products,
            'rows':len(products),'columns':table.colnames,'reported_rows':reported,'truncated':reported>maximum or len(records)>maximum,'spatial_checks':checked,
            'query_region':None if identifier is not None else {'ra_deg':ra,'dec_deg':dec,'radius_deg':radius},
            'coverage_kind':'observation_product_list' if identifier is not None else 'service_footprint_cone_query','transport':transport,
            'scientific_note':'MAST CAOM footprint/cone metadata; listed observations can cover a wide field. Request the selected observation product list. Science/auxiliary flags and public access rights apply; no products were downloaded.'}


def hips_fits_support(record):
    """A FITS container alone cannot turn an RGB preview/cube into a science image.

    只接受声明为二维 image、提供 FITS tile 的 HiPS；不把彩图或立方预览改称科学图。
    """
    formats=re.split(r'[\s,]+',str(record.get('hips_tile_format') or '').lower().strip())
    kind=str(record.get('dataproduct_type') or '').lower()
    if kind!='image' or record.get('hips_cube_depth') not in (None,'',0,'0') or str(record.get('hips_naxis') or '2')!='2':
        return False,'This HiPS is a cube or has unsupported dimensions; select a 2D image survey or an archive product.'
    if str(record.get('dataproduct_subtype') or '').lower()=='color' or 'fits' not in formats:
        return False,'This HiPS provides a visualization preview, not supported 2D FITS tiles; use SDSS, MAST or another archive for science products.'
    return True,'Resampled 2D FITS cutout from HiPS tiles; not an original exposure or a flux-conserving reprojection.'


def hips_priority(record):
    supported,_=hips_fits_support(record)
    name=str(record.get('ID') or '').lower()
    common=next((index for index,part in enumerate(['/dss2/','/sdss','/panstarrs/','/2mass/','/allwise/']) if part in name),10)
    return (not supported,common,name)


def moc_region(field,ra,dec,radius):
    """Use the actual bounded WCS polygon when its spherical interior is known.

    MOC 只用于发现与当前足迹相交的输入；复杂 WCS 的圆锥降级必须明确保守。
    """
    if field.get('field_convex'):
        return {'stc':'Polygon '+' '.join(f'{v:.12f}' for point in field['field_boundary_icrs'] for v in point),'intersect':'overlaps'}
    return {'RA':ra,'DEC':dec,'SR':radius,'intersect':'overlaps'}


def sdss_header_footprints(worker,products,params):
    """Verify a bounded set of SDSS frame footprints from real FITS headers.

    SDSS SIA1 的frame/CRPIX元数据并不总与标准一致，因此不猜交换轴。
    每次最多8个头、每个4MiB wire / 1MiB展开、总45秒；不下载/持久化图像。
    """
    import bz2,time,urllib.request,zlib
    import numpy as np
    from astropy.io import fits
    from astropy.wcs import WCS
    from astropy.wcs.utils import pixel_to_skycoord
    from fitsviewer_archive_geometry import vectors,convex
    from fitsviewer_network import bounded_timeout
    limit=8;wire_limit=4*1024*1024;decoded_limit=1024*1024
    deadline=time.monotonic()+min(45.,bounded_timeout(params.get('timeout',60)));cancel=params.get('_cancel')
    summary={'maximum_headers':limit,'wire_limit_per_header':wire_limit,'wire_limit_total':limit*wire_limit,'decoded_limit_per_header':decoded_limit,'deadline_seconds':45,'checked':0,'wire_bytes':0,'verified':0,'scope':'bounded first returned SDSS products; unverified rows remain diagnostic'}
    for product in products:
        if product.get('footprint'):continue
        url=product.get('access_url');parsed=urllib.parse.urlsplit(url or '')
        if parsed.scheme!='https' or parsed.hostname!='dr17.sdss.org' or not re.fullmatch(r'/sas/dr17/eboss/photoObj/frames/[A-Za-z0-9_./\-]+\.fits(?:\.bz2|\.gz)?',parsed.path) or parsed.query or parsed.fragment:
            product['footprint_error']='No complete provider footprint or allowlisted SDSS frame header';continue
        if summary['checked']>=limit or time.monotonic()>=deadline:
            product['footprint_error']='SDSS header verification budget exhausted; geometry remains unverified';continue
        worker.check_cancelled()
        if cancel and cancel.is_set():raise InterruptedError('Archive footprint verification cancelled')
        summary['checked']+=1
        count=0;body=bytearray();decoder=None;header=None
        try:
            with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'FitsViewer bounded FITS header metadata','Accept-Encoding':'identity','Range':f'bytes=0-{wire_limit-1}'}),timeout=max(.1,min(10.,deadline-time.monotonic()))) as response:
                if urllib.parse.urlsplit(response.geturl()).hostname not in ('dr17.sdss.org','data.sdss.org','data.sdss5.org'):raise ValueError('SDSS frame redirected outside its official data hosts')
                while count<wire_limit:
                    if cancel and cancel.is_set():raise InterruptedError('Archive footprint verification cancelled')
                    if time.monotonic()>=deadline:raise TimeoutError('SDSS header verification deadline exceeded')
                    block=response.read(min(65536,wire_limit-count))
                    if not block:break
                    count+=len(block)
                    if not body and decoder is None:
                        if block.startswith(b'BZh'):decoder=bz2.BZ2Decompressor()
                        elif block.startswith(b'\x1f\x8b'):decoder=zlib.decompressobj(31)
                    if decoder is not None:block=decoder.decompress(block,max(0,decoded_limit-len(body)))
                    body.extend(block)
                    if body and not body.startswith(b'SIMPLE  ='):raise ValueError('SDSS header response is not a primary FITS header')
                    for at in range(0,min(len(body)-79,65536),80):
                        if body[at:at+8]==b'END     ':
                            end=(at//2880+1)*2880
                            if len(body)>=end:header=fits.Header.fromstring(bytes(body[:end]).decode('ascii'));break
                    if header is not None:break
                    if len(body)>=decoded_limit:raise ValueError('SDSS FITS header exceeds the decoded metadata budget')
            if header is None:raise ValueError('No complete FITS header within the bounded SDSS prefix')
            width,height=header.get('NAXIS1'),header.get('NAXIS2')
            if header.get('NAXIS')!=2 or not isinstance(width,int) or not isinstance(height,int) or min(width,height)<2:raise ValueError('SDSS product is not a supported two-dimensional FITS image')
            wcs=WCS(header).celestial
            steps=np.linspace(0,1,9)[:-1]
            pixels=np.vstack((np.column_stack((-.5+steps*width,np.full(8,-.5))),np.column_stack((np.full(8,width-.5),-.5+steps*height)),
                              np.column_stack((width-.5-steps*width,np.full(8,height-.5))),np.column_stack((np.full(8,-.5),height-.5-steps*height))))
            sky=pixel_to_skycoord(pixels[:,0],pixels[:,1],wcs,origin=0).icrs;points=np.column_stack((sky.ra.deg,sky.dec.deg))
            if not convex(vectors(points)):raise ValueError('SDSS header has an unsupported complex footprint')
            product.update(footprint='POLYGON ICRS '+' '.join(f'{v:.14g}' for point in points for v in point),footprint_header_verified=True,
                           footprint_source='official SDSS FITS header; 32 sampled physical pixel-boundary positions',footprint_dimensions=[width,height])
            summary['verified']+=1
        except InterruptedError:raise
        except Exception as error:product['footprint_error']=f'{type(error).__name__}: {error}'
        finally:summary['wire_bytes']+=count
    return summary


def hips_cutout(worker,params):
    """Resolve an exact provider ID before building a bounded official cutout URL."""
    identifier=params.get('hips_id')
    if not isinstance(identifier,str) or not re.fullmatch(r'[A-Za-z0-9._~:/+\-]{1,512}',identifier):raise ValueError('Choose an exact HiPS survey ID from the provider results')
    width,height=params.get('width',512),params.get('height',512)
    if any(isinstance(v,bool) or not isinstance(v,int) or not 16<=v<=4096 for v in (width,height)) or width*height>16*1024*1024:raise ValueError('HiPS cutout dimensions must be 16 to 4096 pixels, at most 16 Mi pixels')
    from fitsviewer_archive_geometry import image_field,vectors,area,overlap_area
    from astropy.wcs import WCS
    from astropy.wcs.utils import pixel_to_skycoord
    import numpy as np
    if not params.get('path'):raise ValueError('Open an image with a valid current WCS footprint before generating a cutout')
    field=image_field(worker,params)
    if params.get('use_current_field',True):
        if not field.get('cutout_wcs'):raise ValueError('The current WCS footprint cannot be bounded by a supported 40%-150% 2D cutout')
        definition=dict(field['cutout_wcs']);old_width,old_height=definition['NAXIS1'],definition['NAXIS2']
        for axis,old,new in [(1,old_width,width),(2,old_height,height)]:
            definition['CRPIX'+str(axis)]=.5+(definition['CRPIX'+str(axis)]-.5)*new/old
            for row in (1,2):definition[f'CD{row}_{axis}']*=old/new
        definition.update(NAXIS1=width,NAXIS2=height)
    else:
        try:ra,dec,fov=float(params['ra_deg']),float(params['dec_deg']),float(params['fov_deg'])
        except (KeyError,TypeError,ValueError):raise ValueError('Provide cutout ICRS RA, Dec and field of view in degrees') from None
        if not all(math.isfinite(v) for v in (ra,dec,fov)) or not -90<=dec<=90 or not .000001<=fov<=3.:raise ValueError('Provide finite ICRS coordinates and a cutout FOV between 0.000001 and 3 degrees')
        step=2*math.tan(math.radians(fov/2))*180/math.pi/max(width,height)
        definition={'NAXIS':2,'NAXIS1':width,'NAXIS2':height,'CTYPE1':'RA---TAN','CTYPE2':'DEC--TAN','CUNIT1':'deg','CUNIT2':'deg','RADESYS':'ICRS','CRVAL1':ra%360,'CRVAL2':dec,'CRPIX1':(width+1)/2,'CRPIX2':(height+1)/2,'CD1_1':-step,'CD1_2':0.,'CD2_1':0.,'CD2_2':step}
    sky=pixel_to_skycoord([-.5,width-.5,width-.5,-.5],[-.5,-.5,height-.5,height-.5],WCS(definition),origin=0).icrs
    shape=vectors(np.column_stack((sky.ra.deg,sky.dec.deg)));target=vectors(field['field_boundary_icrs'])
    ratio=area(shape)/field['field_area_sqdeg'] if field['field_area_sqdeg'] else None
    overlap=overlap_area(shape,target)
    if ratio is None or not .4-1e-9<=ratio<=1.5+1e-9 or overlap is None or overlap<=1e-12*field['field_area_sqdeg']:
        raise ValueError('Cutout must overlap the current image and have 40%-150% of its spherical footprint area')
    failures=[]
    for endpoint in ARCHIVE_PRESETS[0]['urls']:
        worker.check_cancelled()
        try:
            body,transport=fetch_bytes(endpoint,params=dict(moc_region(field,field['ra_deg'],field['dec_deg'],field['radius_deg']),ID=identifier,get='record',fmt='json',MAXREC=2),cancel=params.get('_cancel'),timeout=params.get('timeout',60))
            records=json.loads(body.decode(transport.get('encoding') or 'utf-8-sig'))
            if not isinstance(records,list):raise ServiceError('HiPS provider returned an invalid record list','protocol',endpoint)
            exact=[row for row in records if isinstance(row,dict) and row.get('ID')==identifier]
            if len(exact)!=1:raise ValueError('The exact HiPS survey ID could not be uniquely verified by the provider')
            supported,note=hips_fits_support(exact[0])
            if not supported:raise ValueError(note)
            query={'hips':identifier,'width':width,'height':height,'wcs':json.dumps(definition,separators=(',',':')),'format':'fits'}
            url=endpoint.split('/MocServer/')[0]+'/hips-image-services/hips2fits?'+urllib.parse.urlencode(query)
            return url,{'hips_id':identifier,'provider_verified':True,'cutout':query,'derived':True,'resampled':True,
                        'original_exposure':False,'scientific_note':note,'area_ratio':ratio,'field_area_sqdeg':field['field_area_sqdeg'],
                        'overlap_fraction':overlap/field['field_area_sqdeg'],'discovery_geometry':field['field_geometry'],
                        'area_definition':'candidate spherical footprint area / current image spherical footprint area','prior_service_failures':failures}
        except ServiceError as error:failures.append({'service':endpoint,'error':str(error),'category':error.category})
        except (UnicodeError,json.JSONDecodeError) as error:raise ServiceError('HiPS provider did not return JSON metadata','protocol',endpoint) from error
    raise ServiceError('HiPS metadata verification failed: '+'; '.join(item['error'] for item in failures),failures[-1]['category'])


def search(worker,params):
    require('astropy','numpy')
    from astropy.table import Table
    from fitsviewer_archive_geometry import image_field,select_products
    field=image_field(worker,params) if params.get('path') else {}
    if params.get('use_image_center',True):
        if not field:raise ValueError('Open an image with a valid celestial WCS before searching its current footprint')
        params=dict(params,**{key:field[key] for key in ('ra_deg','dec_deg','radius_deg')});params['frame']='icrs'
    ra,dec,radius=region(params);maximum=max(1,min(int(params.get('max_rows',20)),500))
    preset=next((item for item in ARCHIVE_PRESETS if item['id']==params.get('service_id')),None)
    if preset is None:raise ValueError('Choose a known survey/archive service')
    failures=[];value=None
    for url in preset['urls']:
        worker.check_cancelled()
        try:
            if preset['id']=='mast' and '/api/v0/invoke' in url:
                result=mast_search(worker,params,preset,ra,dec,radius,maximum);result['prior_service_failures']=failures
                all_products=result['products'];selected,selection=select_products(all_products,field)
                result.update(products=selected,rows=len(selected),status='ok' if selected else 'no_qualified_products' if all_products else result['status'],
                              query_region=dict(field,ra_deg=ra,dec_deg=dec,radius_deg=radius),coverage_kind='current_image_footprint',
                              diagnostics={'area_selection':selection,'unselected_products':all_products[:50]})
                return result
            if preset['protocol']=='moc':
                body,transport=fetch_bytes(url,params=dict(moc_region(field,ra,dec,radius),dataproduct_type='image',get='record',fmt='json',MAXREC=2000),cancel=params.get('_cancel'),timeout=params.get('timeout',60))
                try:records=json.loads(body.decode(transport.get('encoding') or 'utf-8-sig'))
                except (ValueError,UnicodeError) as error:raise ServiceError('MOC service did not return JSON records','protocol',url) from error
                if not isinstance(records,list) or any(not isinstance(row,dict) for row in records):raise ServiceError('MOC response has an unexpected record structure','protocol',url)
                if records and not any(row.get('ID') or row.get('hips_service_url') for row in records):raise ServiceError('MOC response has no survey identifiers or preview services','protocol',url)
                products=[];excluded={};supported_records=[]
                # Rank the complete bounded response before truncation, so common
                # science FITS surveys precede visualization-only/cube coverage.
                # 必须先排序完整有界结果再截断，避免默认只显示速度彩图等预览。
                for row in sorted(records,key=hips_priority):
                    supported,cutout_note=hips_fits_support(row)
                    if not supported:excluded[cutout_note]=excluded.get(cutout_note,0)+1;continue
                    if not field.get('cutout_wcs'):excluded['Current image geometry cannot produce a verified 40%-150% cutout']=excluded.get('Current image geometry cannot produce a verified 40%-150% cutout',0)+1;continue
                    supported_records.append(row)
                for i,row in enumerate(supported_records[:maximum]):
                    _,cutout_note=hips_fits_support(row)
                    products.append({'row':i,'source':preset['label'],'service_id':preset['id'],'collection':row.get('obs_collection'),
                                     'id':row.get('ID'),'title':str(row.get('obs_title') or row.get('ID'))+' — current image cutout','product_type':'current-field cutout / mosaic','processing':'resampled HiPS tiles; not an original exposure',
                                     'access_url':row.get('hips_service_url'),'access_format':row.get('hips_tile_format'),
                                     'download_kind':'hips_cutout','coverage':'current_image_cutout','semantics':'A bounded 2D FITS cutout of the current image footprint, generated on request; resampled, not an original exposure',
                                     'hips_id':row.get('ID'),'cutout_supported':True,'cutout_note':cutout_note,
                                     'cutout_width':field['cutout_wcs']['NAXIS1'],'cutout_height':field['cutout_wcs']['NAXIS2'],
                                     'area_ratio':field['cutout_area_ratio'],'field_area_sqdeg':field['field_area_sqdeg'],'area_selection':'preferred_40_to_150_percent',
                                     'overlap_fraction':field['cutout_overlap_fraction'],'discovery_geometry':field['field_geometry'],
                                     'hips_tile_format':row.get('hips_tile_format'),'hips_cube_depth':row.get('hips_cube_depth'),
                                     'regime':row.get('obs_regime'),'copyright':row.get('obs_copyright'),'provenance_url':row.get('obs_progenitor_url')})
                return {'status':'ok' if products else 'no_qualified_products' if records else 'empty','service_id':preset['id'],'service':url,'products':products,'rows':len(products),
                        'reported_rows':len(records),'truncated':len(supported_records)>maximum or len(records)>=2000,'query_region':dict(field,ra_deg=ra,dec_deg=dec,radius_deg=radius),
                        'coverage_kind':'current_image_cutouts','prior_service_failures':failures,'transport':transport,'diagnostics':{'coverage_records':len(records),'excluded_coverage_records':excluded},
                        'scientific_note':'Only bounded cutouts of the current image footprint are listed. Survey coverage is internal discovery metadata; output images are resampled HiPS mosaics, not original exposures.'}
            options={'service':url,'protocol':preset['protocol'],'max_rows':maximum,'timeout':params.get('timeout',60),'_cancel':params.get('_cancel'),'_file_capability':file_capability_request(worker,params)}
            if preset['protocol']=='tap':
                geometry=(f"CONTAINS(POINT('ICRS',{ra:.12f},{dec:.12f}),s_region)=1" if preset.get('center_coverage') else
                          f"INTERSECTS(CIRCLE('ICRS',{ra:.12f},{dec:.12f},{radius:.12f}),s_region)=1")
                columns='obs_collection,obs_id,obs_title,facility_name,instrument_name,dataproduct_type,calib_level,s_ra,s_dec,s_region,t_min,t_max,em_min,em_max,access_url,access_format,access_estsize'
                columns=','.join(name for name in columns.split(',') if name not in preset.get('unsupported_columns',[]))
                options['adql']=f"SELECT TOP {maximum} {columns} FROM {preset['table']} WHERE {geometry}"
            else:options.update(ra_deg=ra,dec_deg=dec,size_deg=radius*2,format='image/fits')
            collection=params['collection'] if 'collection' in params else preset.get('default_collection')
            if collection:options['collection']=str(collection)[:128]
            value=query(worker,options);break
        except ServiceError as error:
            failures.append({'service':url,'error':str(error),'category':error.category})
            if preset['id']=='mast' and params.get('observation_id') is not None:raise
    if value is None:raise ServiceError('Archive interfaces failed: '+'; '.join(item['error'] for item in failures),failures[-1]['category'])
    table=read_table(value['_path'],format='votable',file_capability=file_capability_request(worker,params));products=product_rows(table,preset)
    header_checks=sdss_header_footprints(worker,products,params) if preset['id']=='sdss_dr17' and field else None
    checked=check_products(products,ra,dec,radius)
    all_products=products;products,selection=select_products(products,field)
    status='ok' if products else 'no_qualified_products' if all_products else value['status']
    return dict(value,status=status,service_id=preset['id'],products=products,rows=len(products),prior_service_failures=failures,spatial_checks=checked,selected_collection=params.get('collection',preset.get('default_collection')),
                query_region=dict(field,ra_deg=ra,dec_deg=dec,radius_deg=radius),coverage_kind='current_image_footprint',
                diagnostics={'area_selection':selection,'header_verification':header_checks,'unselected_products':[row for row in all_products if row not in products][:50]},
                scientific_note=preset['product_scope']+'. Metadata search does not download or validate a product. Spatial coverage is reported by the service; inspect footprints and select science/weight/mask and processing level explicitly.')


def datalink(worker,params):
    require('astropy','numpy')
    import xml.etree.ElementTree as ET
    url=http_url(params['url']);maximum=max(1,min(int(params.get('max_rows',100)),1000))
    body,transport=fetch_bytes(url,cancel=params.get('_cancel'),timeout=params.get('timeout',60))
    table,parsed=parse_votable(body,file_capability=file_capability_request(worker,params));transport.update(parsed)
    names={name.lower():name for name in table.colnames}
    if not {'id','access_url','semantics','error_message'}<=set(names):raise ServiceError('Response is not an IVOA DataLink links table','protocol',url)
    def cell(row,name):return scalar(row[names[name]]) if name in names else None
    links=[]
    for row in table[:maximum]:
        access=cell(row,'access_url');error=cell(row,'error_message');service=cell(row,'service_def');mime=cell(row,'content_type')
        if access:
            try:access=http_url(str(access))
            except ValueError:access=None;error=error or 'Unsupported access URL'
        length=cell(row,'content_length')
        try:length=int(length) if length is not None else None
        except (TypeError,ValueError):length=None
        if length is not None and length<0:length=None
        kind='datalink' if 'datalink' in str(mime).lower() else 'fits' if ('fits' in str(mime).lower() or any(part in str(access).lower() for part in ['.fits','.fit','.fts','.fz'])) else 'text' if str(mime).lower().startswith('text/plain') or 'csv' in str(mime).lower() else 'other'
        links.append({'id':cell(row,'id'),'access_url':access,'semantics':cell(row,'semantics'),'description':cell(row,'description'),
                      'content_type':mime,'content_length':length,'service_def':service,'error_message':error,
                      'downloadable':bool(access and not error and not service and kind in ['fits','text']),'download_kind':kind,
                      'requires_service':bool(service),'recursive_datalink':'datalink' in str(mime).lower()})
    descriptors=[]
    try:
        root=ET.fromstring(body)
        for resource in root.iter():
            if resource.tag.rsplit('}',1)[-1]!='RESOURCE' or resource.attrib.get('utype','').lower()!='adhoc:service':continue
            values={};inputs=[]
            for node in resource:
                kind=node.tag.rsplit('}',1)[-1]
                if kind=='PARAM':values[node.attrib.get('name','')]=node.attrib.get('value','')
                if kind=='GROUP' and node.attrib.get('name')=='inputParams':
                    inputs=[dict(child.attrib) for child in node if child.tag.rsplit('}',1)[-1]=='PARAM']
            descriptors.append({'id':resource.attrib.get('ID'),'access_url':values.get('accessURL'),'standard_id':values.get('standardID'),'input_parameters':inputs})
    except ET.ParseError:pass
    return {'status':'ok' if links else 'empty','service':url,'links':links,'rows':len(links),'reported_rows':len(table),
            'truncated':len(table)>maximum,'service_descriptors':descriptors,'transport':transport,
            'scientific_note':'Choose a specific science/auxiliary product. Service descriptors require explicit parameters; private products and error rows are not downloadable.'}
