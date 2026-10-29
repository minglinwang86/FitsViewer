"""Exercise the real bounded image HTTP API against Astropy/CFITSIO samples.

Uses a private temporary server and synthetic numerical fixtures, optionally one
existing real template. Does not restart Jupyter or modify source FITS files.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import struct
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request

import numpy as np
from astropy.io import fits


def packet(body):
    length = struct.unpack('<I', body[:4])[0]
    info = json.loads(body[4:4 + length])
    types = {8: 'u1', 16: '<i2', 32: '<i4', 64: '<i8', -32: '<f4', -64: '<f8'}
    raw = np.frombuffer(body, dtype=types[info['bitpix']], offset=4 + length).reshape(info['height'], info['width'])
    values = raw.astype(np.float64) * info['bscale'] + info['bzero']
    if info.get('blank') is not None:
        values[raw == info['blank']] = np.nan
    return info, values


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--web', type=Path, required=True)
    parser.add_argument('--real-template', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    checks = []
    with tempfile.TemporaryDirectory(prefix='fv-image-api-') as directory:
        root = Path(directory)
        data = np.arange(2000 * 2000, dtype=np.float64).reshape(2000, 2000)
        data[0, 0] = np.nan
        fits.writeto(root / 'double.fits', data)
        del data
        rows = np.broadcast_to(np.arange(6000, dtype=np.int16), (6000, 6000)).copy()
        fits.HDUList([fits.PrimaryHDU(), fits.CompImageHDU(rows, compression_type='RICE_1')]).writeto(root / 'large-rice.fits.fz')
        del rows
        for codec in ['GZIP_2', 'RICE_1']:
            yy, xx = np.ogrid[:2000, :4000]
            samples = (np.sin(xx / 11.) + np.cos(yy / 17.)).astype(np.float32)
            samples[10, 5] = np.nan
            # Multi-row source tiles exercise random-table wrap (>10000 pixels)
            # and partial edges independently of the 256-pixel display tiling.
            options = dict(compression_type=codec, quantize_level=16 if codec == 'RICE_1' else 0)
            if codec == 'RICE_1':
                options.update(tile_shape=(71, 200), quantize_method=2, dither_seed=37)
            fits.HDUList([fits.PrimaryHDU(), fits.CompImageHDU(samples, **options)]).writeto(root / f'float-{codec}.fits.fz')
        del samples, yy, xx
        files = list(root.glob('*.fits*'))
        if args.real_template:
            shutil.copyfile(args.real_template, root / 'real-template.fits')
            files.append(root / 'real-template.fits')
        identities = {path.name: hashlib.sha256(path.read_bytes()).hexdigest() for path in files}
        with socket.socket() as listener:
            listener.bind(('127.0.0.1', 0))
            port = listener.getsockname()[1]
        runtime = root / 'runtime'
        runtime.mkdir()
        env = dict(os.environ, FITSVIEWER_RUNTIME_DIR=str(runtime))
        with (root / 'server.log').open('wb') as log:
            process = subprocess.Popen([str(args.binary.resolve()), '--serve', '--root', str(root), '--web-dir', str(args.web.resolve()), '--port', str(port), '--no-browser'], env=env, stdout=log, stderr=subprocess.STDOUT)
            try:
                deadline = time.monotonic() + 20
                while not (runtime / 'server.json').exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError('Private image server did not start')
                    time.sleep(.05)
                token = json.loads((runtime / 'server.json').read_text())['token']

                def get(operation, **params):
                    url = f'http://127.0.0.1:{port}/api/image/{operation}?' + urllib.parse.urlencode(params)
                    request = urllib.request.Request(url, headers={'X-Fitsviewer-Token': token})
                    with urllib.request.urlopen(request, timeout=30) as response:
                        return response.read()

                for path in files:
                    started = time.monotonic()
                    metadata = json.loads(get('metadata', path=path.name))
                    common = dict(path=path.name, hdu=metadata['info']['hdu'], version=metadata['source_version'])
                    checks.append({'file': path.name, 'transport': metadata['transport'], 'metadata_seconds': round(time.monotonic() - started, 3)})
                    if path.name == 'real-template.fits' and metadata['transport'] == 'full':
                        info, values = packet(get('full', **common))
                        with fits.open(path, memmap=True) as hdus:
                            np.testing.assert_allclose(values, hdus[info['hdu']].data, rtol=0, atol=0, equal_nan=True)
                        checks[-1]['original_values_match'] = True
                        continue
                    assert metadata['transport'] == 'tiles', metadata
                    with fits.open(path, memmap=True) as hdus:
                        image = hdus[metadata['info']['hdu']]
                        width, height = metadata['info']['width'], metadata['info']['height']
                        edge = (0, (width-1)//256, (height-1)//256)
                        for level, tx, ty in [(metadata['preview_level'], 0, 0), (0, 1, 1), edge]:
                            step = 2 ** level
                            x = tx * 256 * step
                            y = ty * 256 * step
                            original = get('tile', **common, level=level, x=tx, y=ty, display_bits=0)
                            info, values = packet(original)
                            xs = np.minimum(x + np.arange(info['width']) * step + step // 2, width - 1)
                            ys = np.minimum(y + np.arange(info['height']) * step + step // 2, height - 1)
                            # Each row uses the independent FITS decoder. No Rust-rendered
                            # preview or quantized result is reused as a reference.
                            expected = np.stack([image.section[int(sy), :][xs] for sy in ys])
                            np.testing.assert_allclose(values, expected, rtol=1e-6, atol=1e-6, equal_nan=True)
                            compact = get('tile', **common, level=level, x=tx, y=ty, display_bits=16)
                            compact_info, compact_values = packet(compact)
                            np.testing.assert_array_equal(np.isfinite(compact_values), np.isfinite(expected))
                            tolerance = abs(compact_info['bscale']) * .51 + np.nanmax(np.abs(expected)) * np.finfo(float).eps * 2
                            assert np.nanmax(np.abs(compact_values - expected)) <= tolerance
                            if metadata['info']['bitpix'] == -64:
                                assert len(compact) < len(original) * .3
                        readout = json.loads(get('pixel', **common, x=17, y=25))
                        expected = float(image.section[25:26, 17:18][0, 0])
                        assert readout['original_samples'] is True
                        np.testing.assert_allclose(readout['value'], expected, rtol=1e-6, atol=1e-6)
                    checks[-1].update(original_tiles_match=True, compact_error_bounded=True, exact_readout_matches=True)
                for path in files:
                    assert hashlib.sha256(path.read_bytes()).hexdigest() == identities[path.name]
            finally:
                if process.poll() is None:
                    process.send_signal(signal.SIGINT) if os.name != 'nt' else process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait(timeout=5)
                assert process.poll() is not None
    result = {'checks': checks, 'source_files_unchanged': True, 'owned_server_reaped': True, 'scope': 'Synthetic numerics plus the explicitly supplied real template, not complete GUI acceptance'}
    args.output.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
