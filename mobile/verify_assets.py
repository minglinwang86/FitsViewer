"""Validate compiled mobile profile and exact asset hashes before native wrapping."""
import hashlib
import base64
import json
import re
from pathlib import Path
import sys

def verify(directory):
    directory=Path(directory).resolve()
    value=json.loads((directory/'build-profile.json').read_text())
    if value['modules']!=['base'] or value['pro'] or value['wasm_features']!=['offline'] or not value['will_execute']:
        raise ValueError('Mobile requires an executed base-only offline WASM build')
    actual={p.name for p in directory.iterdir()}
    if actual!=set(value['files'])|{'build-profile.json'}:
        raise ValueError('Unrecognized extra mobile resource')
    for name,item in value['files'].items():
        if name=='build-profile.json':
            continue
        if Path(name).name!=name:
            raise ValueError('Unexpected asset path')
        data=(directory/name).read_bytes()
        if len(data)!=item['bytes'] or hashlib.sha256(data).hexdigest()!=item['sha256']:
            raise ValueError('Asset hash mismatch: '+name)
    if not {'index.html','icon.png','manifest.webmanifest','sw.js'}.issubset(value['files']):
        raise ValueError('Incomplete mobile resources')
    embedded=[base64.b64decode(item) for item in re.findall(r"bytes\('([A-Za-z0-9+/=]+)'\)",(directory/'index.html').read_text())]
    for name,item in value['inline_resources'].items():
        if not any(len(data)==item['bytes'] and hashlib.sha256(data).hexdigest()==item['sha256'] for data in embedded):
            raise ValueError('Embedded resource mismatch: '+name)
    if not value['embedded_license_inventory']:
        raise ValueError('Missing embedded license inventory')
    return value

if __name__=='__main__':
    result=verify(sys.argv[1])
    print(json.dumps({'verified':True,'modules':result['modules'],'source_commit':result.get('source_commit')}))
