"""Reuse the exact compiled APK for a document-picker harness repair only."""
import argparse,hashlib,json,os,zipfile
from pathlib import Path,PurePosixPath
from reuse_web import api,archive,unpack
from verify_assets import verify

RUN=40978836932
SOURCE='3f3164002b44e154c911711bc623598cfbcfb472'
JOB=123815494384
ARTIFACT=12944018540
ZIP_SHA='6db9fbdbc5659fe8971aa0a0f4bebfa1fc3dd1e5a4bd9ea916cdcd39d4fd28f6'
APK_SHA='917552c1062933a02f6fb787f1a6046b90852c04cd9ee413df8945912e5d49ef'

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    token=os.environ['GH_TOKEN'];head=os.environ['GITHUB_SHA'];checkout=Path(__file__).resolve().parents[1]
    if os.environ.get('GITHUB_REPOSITORY')!='minglinwang86/FitsViewer':raise ValueError('Unexpected repository')
    run=api(f'actions/runs/{RUN}',token);job=api(f'actions/jobs/{JOB}',token)
    if run['head_sha']!=SOURCE or job['run_id']!=RUN or job['head_sha']!=SOURCE or job['name']!='android':raise ValueError('Original Android source/job mismatch')
    if not any(step['name']=='Build a debug-signed sideload test APK' and step['conclusion']=='success' for step in job['steps']):raise ValueError('Original APK build and signature step did not pass')
    comparison=api(f'compare/{SOURCE}...{head}',token);changes=comparison.get('files',[])
    allowed={'.github/workflows/mobile-mini.yml','mobile/qa_android.cjs','mobile/reuse_android.py'}
    if comparison.get('status') not in ('ahead','identical') or len(changes)>3 or any(item['filename'] not in allowed or item['status'] not in ('added','modified') for item in changes):raise ValueError('Application inputs changed; exact APK reuse is prohibited')
    metadata=api(f'actions/artifacts/{ARTIFACT}',token)
    if metadata['name']!='FitsViewer-Mini-Android-0.1.7-test' or metadata['expired'] or metadata['digest']!='sha256:'+ZIP_SHA or metadata['workflow_run']['id']!=RUN or metadata['workflow_run']['head_sha']!=SOURCE:raise ValueError('Original artifact identity/digest mismatch')
    data=archive(ARTIFACT,token)
    if len(data)!=metadata['size_in_bytes'] or hashlib.sha256(data).hexdigest()!=ZIP_SHA:raise ValueError('Original artifact ZIP bytes differ')
    output=args.output.resolve();output.mkdir(parents=True,exist_ok=False)
    original=output/'original.zip';original.write_bytes(data);unpack(original,output/'original')
    candidates=list((output/'original').rglob('app-debug.apk'));signatures=list((output/'original').rglob('apk-signature.txt'))
    if len(candidates)!=1 or len(signatures)!=1:raise ValueError('Unexpected APK artifact members')
    apk=candidates[0].read_bytes();signature=signatures[0].read_text()
    if hashlib.sha256(apk).hexdigest()!=APK_SHA or 'Verifies' not in signature or 'Signer #1 certificate SHA-256 digest:' not in signature:raise ValueError('Accepted APK/signature report differs')
    site=output/'site';site.mkdir()
    with zipfile.ZipFile(candidates[0]) as stream:
        for entry in stream.infolist():
            if not entry.filename.startswith('assets/') or entry.is_dir():continue
            name=entry.filename[len('assets/'):]
            if PurePosixPath(name).name!=name:raise ValueError('Unexpected nested app resource')
            (site/name).write_bytes(stream.read(entry))
    profile=verify(site)
    if profile['source_commit']!=SOURCE:raise ValueError('APK embedded source mismatch')
    for name,digest in profile['source_hashes'].items():
        relative=PurePosixPath(name)
        if relative.is_absolute() or '..' in relative.parts or hashlib.sha256((checkout/name).read_bytes()).hexdigest()!=digest:raise ValueError('Accepted APK source hash differs: '+name)
    (output/'app-debug.apk').write_bytes(apk);(output/'apk-signature.txt').write_text(signature)
    receipt={'original_run':RUN,'original_source':SOURCE,'original_job':JOB,'artifact':ARTIFACT,'zip_sha256':ZIP_SHA,'apk_sha256':APK_SHA,'repair_commit':head,'application_source_unchanged':True,'changes':[item['filename'] for item in changes],'source_files_verified':len(profile['source_hashes']),'wasm_sha256':profile['inline_resources']['wasm']['sha256']}
    (output/'reuse-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt))

if __name__=='__main__':main()
