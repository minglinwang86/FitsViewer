"""Measure multi-document memory in an isolated Jupyter server. / 隔离测试多文件内存。"""
from pathlib import Path
import argparse
import array
import gzip
import hashlib
import json
import os
import random
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time

root = Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--python', required=True, help='Python in an isolated environment containing the candidate wheel')
parser.add_argument('--wheel', required=True, type=Path)
parser.add_argument('--label', default='candidate')
parser.add_argument('--count', type=int, default=6)
parser.add_argument('--size', type=int, default=2048)
parser.add_argument('--cycles', type=int, default=4)
parser.add_argument('--probe', action='store_true', help='Inspect which installed frontend is actually loaded')
parser.add_argument('--control', action='store_true', help='Compare ordinary Jupyter text editors without FITS/WASM')
parser.add_argument('--lightweight-cdp', action='store_true', help='Repeat without Playwright page/network response instrumentation')
parser.add_argument('--uncompressed', action='store_true', help='Use ordinary .fits files instead of gzip FITS')
args = parser.parse_args()
assert 1 <= args.count <= 8 and 256 <= args.size <= 2048 and 1 <= args.cycles <= 8
assert args.label.replace('-', '').replace('_', '').isalnum()
for name in ['logs', 'reports']: (root / name).mkdir(exist_ok=True)
server, runtime = None, None
with tempfile.TemporaryDirectory(prefix='fitsview-memory-') as temporary:
    scratch = Path(temporary)
    data = scratch / 'data'; data.mkdir()
    files = []
    def card(key, value): return f'{key:<8}= {value:>20}'.ljust(80).encode('ascii')
    for number in range(args.count):
        header = b''.join([card('SIMPLE', 'T'), card('BITPIX', '-32'), card('NAXIS', '2'),
                           card('NAXIS1', str(args.size)), card('NAXIS2', str(args.size)), b'END'.ljust(80)])
        header += b' ' * (-len(header) % 2880)
        rng = random.Random(number)
        row = array.array('f', [100 + 15 * rng.random() + x / args.size for x in range(args.size)])
        if sys.byteorder == 'little': row.byteswap()
        path = data / (f'memory_{number+1:02}.fits' + ('' if args.uncompressed else '.gz'))
        opener = open if args.uncompressed else gzip.open
        with opener(path, 'wb') as stream:
            stream.write(header)
            for _ in range(args.size): stream.write(row.tobytes())
            stream.write(b'\0' * (-(args.size * args.size * 4) % 2880))
        if args.control:
            path = data / f'control_{number+1:02}.txt'
            path.write_text('Memory control: ordinary Jupyter editor.\n' * 3)
        files.append(path.name)
    env = {k: v for k, v in os.environ.items() if not k.startswith(('FITSVIEW_', 'JUPYTER_', 'PYTHON'))}
    env.update(PATH=str(Path(args.python).absolute().parent) + os.pathsep + os.environ['PATH'],
               XDG_CACHE_HOME=str(scratch / 'cache'), XDG_CONFIG_HOME=str(scratch / 'settings'), XDG_DATA_HOME=str(scratch / 'desktop'),
               JUPYTER_CONFIG_DIR=str(scratch / 'config'), JUPYTER_DATA_DIR=str(scratch / 'jupyter'),
               JUPYTER_RUNTIME_DIR=str(scratch / 'runtime'), JUPYTER_PREFER_ENV_PATH='1')
    runtime_dir = subprocess.check_output([args.python, '-c', 'from fitsview_jupyter.runtime import runtime_dir; print(runtime_dir())'], env=env, text=True).strip()
    with socket.socket() as probe: probe.bind(('127.0.0.1', 0)); port = probe.getsockname()[1]
    command = [args.python, '-m', 'jupyterlab', '--no-browser', '--allow-root', '--ip=127.0.0.1', f'--port={port}',
               '--ServerApp.port_retries=0', '--ServerApp.base_url=/memory-test/', '--LabApp.expose_app_in_browser=True',
               "--ServerApp.jpserver_extensions={'fitsview_jupyter':True,'jupyter_server_proxy':True}", f'--ServerApp.root_dir={data}']
    try:
        with (root / 'logs' / ('memory-' + args.label + '-jupyter.log')).open('wb') as log:
            server = subprocess.Popen(command, env=env, stdout=log, stderr=subprocess.STDOUT)
        info = Path(env['JUPYTER_RUNTIME_DIR']) / f'jpserver-{server.pid}.json'
        for _ in range(120):
            if info.exists(): break
            if server.poll() is not None: raise RuntimeError('Test server failed; see memory Jupyter log')
            time.sleep(.5)
        assert info.exists()
        config = {'server_info': str(info), 'rust_state': str(Path(runtime_dir) / 'server.json'), 'probe':args.probe, 'control':args.control, 'uncompressed':args.uncompressed,
                  'files': files, 'size': args.size, 'cycles': args.cycles, 'label': args.label,
                  'wheel': args.wheel.name, 'sha256': hashlib.sha256(args.wheel.read_bytes()).hexdigest()}
        config_file = scratch / 'config.json'; config_file.write_text(json.dumps(config))
        node = shutil.which('node') or str(root / '.tools/node/bin/node')
        driver = 'verify_memory_light.cjs' if args.lightweight_cdp else 'verify_memory.cjs'
        subprocess.run([node, str(root / 'scripts' / driver), str(config_file)], check=True, timeout=600)
    finally:
        state = Path(runtime_dir) / 'server.json'
        if state.exists():
            runtime = json.loads(state.read_text())
            exe = Path(f"/proc/{runtime['pid']}/exe")
            if exe.exists() and '_vendor/bin/fitsview' in str(exe.resolve()):
                os.kill(runtime['pid'], signal.SIGTERM)
        if server and server.poll() is None:
            server.terminate()
            try: server.wait(timeout=15)
            except subprocess.TimeoutExpired: server.kill(); server.wait()
