"""Reuse one pinned successful Mini web build without compiling or rerunning its tests."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import urllib.error
import urllib.parse
import urllib.request
import zipfile
from verify_assets import verify

REPO='minglinwang86/FitsViewer'
RUN=40969033732
SOURCE='7bfc3d358fc60d556f0c992880f9fa3420847d38'
WEB_JOB=123780797112
IOS_JOB=123782357590
ARTIFACTS={
    'bundle':(12940275725,'fitsviewer-mini-web-0.1.7','c7249e11c2151116549f9d256a3d4de9beee005eb6aa51f3ec7e86c8b8c37dba'),
    'evidence':(12939956589,'mobile-browser-evidence','7c7f94418132d337008f4b11c9d748afa7259672cc98db40db3e9a62ce90e88e'),
}

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self,*args,**kwargs):
        return None

def request_api(path,token):
    return urllib.request.Request('https://api.github.com/repos/'+REPO+'/'+path,headers={
        'Authorization':'Bearer '+token,'Accept':'application/vnd.github+json','X-GitHub-Api-Version':'2022-11-28'})

def read_limited(response,limit=32*1024*1024):
    data=response.read(limit+1)
    if len(data)>limit:raise ValueError('Pinned API/artifact response exceeds the bounded download limit')
    return data

def api(path,token):
    with urllib.request.urlopen(request_api(path,token),timeout=60) as response:
        return json.loads(read_limited(response))

def archive(artifact_id,token):
    # Follow the signed artifact URL without forwarding the GitHub bearer token.
    opener=urllib.request.build_opener(NoRedirect)
    try:
        with opener.open(request_api(f'actions/artifacts/{artifact_id}/zip',token),timeout=60) as response:
            return read_limited(response)
    except urllib.error.HTTPError as error:
        if error.code not in (301,302,303,307,308):raise
        location=error.headers['Location']
        if urllib.parse.urlparse(location).scheme!='https':raise ValueError('Artifact redirect must use HTTPS')
        with urllib.request.urlopen(location,timeout=60) as response:
            return read_limited(response)

def unpack(archive_path,output):
    output.mkdir(parents=True,exist_ok=False)
    with zipfile.ZipFile(archive_path) as stream:
        if len(stream.infolist())>256:raise ValueError('Unexpected artifact entry count')
        total=0
        for entry in stream.infolist():
            relative=PurePosixPath(entry.filename)
            if relative.is_absolute() or '..' in relative.parts or '\\' in entry.filename:
                raise ValueError('Unsafe artifact member path')
            if (entry.external_attr>>16)&0o170000==0o120000:
                raise ValueError('Artifact symlinks are not accepted')
            total+=entry.file_size
            if total>64*1024*1024:raise ValueError('Expanded artifact exceeds the limit')
        stream.extractall(output)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    token=os.environ['GH_TOKEN']
    checkout=Path(__file__).resolve().parents[1]
    head=os.environ['GITHUB_SHA']
    if os.environ.get('GITHUB_REPOSITORY')!=REPO:raise ValueError('Unexpected repository')
    run=api(f'actions/runs/{RUN}',token)
    if run['head_sha']!=SOURCE:raise ValueError('Pinned source run mismatch')
    for job_id,name in [(WEB_JOB,'web-mini'),(IOS_JOB,'ios')]:
        job=api(f'actions/jobs/{job_id}',token)
        if job['run_id']!=RUN or job['head_sha']!=SOURCE or job['name']!=name or job['conclusion']!='success':
            raise ValueError('Pinned accepted job is not successful: '+name)
    comparison=api(f'compare/{SOURCE}...{head}',token)
    changes=comparison.get('files',[])
    if comparison.get('status') not in ('ahead','identical') or len(changes)>3:
        raise ValueError('Unexpected source ancestry/diff; use workflow_dispatch for a fresh build')
    allowed={'.github/workflows/mobile-mini.yml','mobile/reuse_web.py','mobile/qa_android.cjs'}
    if any(item['filename'] not in allowed or item['status'] not in ('added','modified') for item in changes):
        raise ValueError('Application inputs changed; use workflow_dispatch instead of artifact reuse')
    output=args.output.resolve()
    if output.exists():raise FileExistsError('Use a clean task output directory')
    output.mkdir(parents=True)
    receipt={'original_run':RUN,'original_source':SOURCE,'repair_commit':head,'application_source_unchanged':True,'test_harness_changes':[item['filename'] for item in changes if item['filename']=='mobile/qa_android.cjs'],'accepted_jobs':{'web':WEB_JOB,'ios':IOS_JOB},'artifacts':{}}
    for kind,(artifact_id,name,digest) in ARTIFACTS.items():
        metadata=api(f'actions/artifacts/{artifact_id}',token)
        if metadata['id']!=artifact_id or metadata['name']!=name or metadata['expired'] or metadata['digest']!='sha256:'+digest or metadata['workflow_run']['id']!=RUN or metadata['workflow_run']['head_sha']!=SOURCE:
            raise ValueError('Pinned artifact identity/digest mismatch: '+kind)
        data=archive(artifact_id,token)
        if len(data)!=metadata['size_in_bytes'] or hashlib.sha256(data).hexdigest()!=digest:
            raise ValueError('Downloaded original ZIP digest mismatch: '+kind)
        archive_path=output/(kind+'.zip');archive_path.write_bytes(data)
        unpack(archive_path,output/('site' if kind=='bundle' else 'browser-evidence'))
        receipt['artifacts'][kind]={'id':artifact_id,'name':name,'zip_sha256':digest,'bytes':len(data)}
    metadata=verify(output/'site')
    if metadata['source_commit']!=SOURCE:raise ValueError('Bundle source commit mismatch')
    for name,digest in metadata['source_hashes'].items():
        relative=PurePosixPath(name)
        if relative.is_absolute() or '..' in relative.parts:raise ValueError('Unsafe source hash path')
        if hashlib.sha256((checkout/name).read_bytes()).hexdigest()!=digest:
            raise ValueError('Accepted bundle source hash differs: '+name)
    candidates=list((output/'browser-evidence').rglob('result.json'))
    if len(candidates)!=1:raise ValueError('Expected one accepted browser delta result')
    evidence=json.loads(candidates[0].read_text())
    checks=evidence['checks']
    if len(checks)!=2 or {(item['width'],item['height'],item['dpr']) for item in checks}!={(390,844,2),(844,390,2)}:
        raise ValueError('Accepted browser viewport evidence is incomplete')
    for item in checks:
        if item['errors'] or item['hduReturnsOriginal']!=[11,111,22,222,111] or item['tabCount']!=2:
            raise ValueError('Accepted local-source/HDU delta check failed')
        if not all(item[key] is True for key in ['replaceKeepsOtherTab','touchZoom','titleLog','detailsMenu','proExcluded']):
            raise ValueError('Accepted mobile delta check did not pass')
        if len(set(item['opaqueSameNames']))!=3 or not all(name.startswith('local://') for name in item['opaqueSameNames']):
            raise ValueError('Opaque local identity evidence mismatch')
    receipt['accepted_delta_json_sha256']=hashlib.sha256(candidates[0].read_bytes()).hexdigest()
    receipt['verified_source_files']=len(metadata['source_hashes'])
    (output/'reuse-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
    print(json.dumps(receipt,indent=2))

if __name__=='__main__':
    main()
