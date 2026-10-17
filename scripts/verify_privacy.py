"""Inspect release archives, including embedded binaries, for private markers.

检查发行归档及其中的二进制文件，不输出匹配到的敏感内容。
"""
from pathlib import Path
import argparse
import gzip
import io
import json
import re
import tarfile
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('artifacts', nargs='+', type=Path)
parser.add_argument('--markers', type=Path, help='Private JSON list of forbidden strings; keep outside release sources')
parser.add_argument('--report', type=Path, help='Write a report containing locations, never matched secret values')
args = parser.parse_args()
markers = json.loads(args.markers.read_text()) if args.markers else []
assert isinstance(markers, list) and all(isinstance(s, str) and s for s in markers)
encoded = [(s.encode(), s.encode('utf-16le')) for s in markers]
findings, inspected = [], 0
# Generic credential/profile rules supplement the user's private marker list.
# 通用规则补查凭据和个人目录，第三方许可证中的原作者署名无需删除。
patterns = [
    ('windows_profile', re.compile(rb'(?i)[a-z]:[\\/]Users[\\/](?!Public[\\/]|Default[\\/]|user[\\/]|example[\\/])[^\s\x00/\\"<>]{2,}[\\/]')),
    ('private_key', re.compile(rb'-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----')),
    ('github_token', re.compile(rb'(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,})')),
    ('private_ipv4', re.compile(rb'(?<![\d.])(?:10(?:\.\d{1,3}){3}|192\.168(?:\.\d{1,3}){2}|172\.(?:1[6-9]|2\d|3[01])(?:\.\d{1,3}){2})(?![\d.])')),
]

def inspect(name, data, depth=0):
    global inspected
    inspected += 1
    for number, pair in enumerate(encoded):
        if any(value in data for value in pair):
            findings.append({'member': name, 'rule': 'private_marker', 'marker_index': number})
    for rule, pattern in patterns:
        if pattern.search(data): findings.append({'member': name, 'rule': rule})
    if depth >= 4: return
    stream = io.BytesIO(data)
    if zipfile.is_zipfile(stream):
        with zipfile.ZipFile(stream) as archive:
            for item in archive.infolist():
                if not item.is_dir(): inspect(name + '!' + item.filename, archive.read(item), depth + 1)
    elif name.endswith(('.tar.gz', '.tgz', '.tar')):
        with tarfile.open(fileobj=stream, mode='r:*') as archive:
            for item in archive:
                if item.isfile(): inspect(name + '!' + item.name, archive.extractfile(item).read(), depth + 1)
    elif name.endswith('.gz'):
        inspect(name + '!decompressed', gzip.decompress(data), depth + 1)

for artifact in args.artifacts: inspect(artifact.name, artifact.read_bytes())
report = {'passed': not findings, 'artifacts': [p.name for p in args.artifacts],
          'members_inspected': inspected, 'private_marker_count': len(markers), 'findings': findings}
if args.report:
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
raise SystemExit(0 if report['passed'] else 1)
