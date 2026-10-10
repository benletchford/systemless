#!/usr/bin/env python3
"""Reject changed restoration pixels even with valid provenance hashes."""
import argparse, copy, hashlib, importlib.util, json, pathlib, sys, tempfile
from PIL import Image
sys.dont_write_bytecode = True
script = pathlib.Path(__file__).with_name('verify-gpui-styled-activation-pixels.py')
spec = importlib.util.spec_from_file_location('activation', script)
verifier = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verifier)
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('source', type=pathlib.Path)
parser.add_argument('--report', type=pathlib.Path)
args = parser.parse_args()
base = json.loads((args.source / 'progress.json').read_text())
rejected = []
for kind, before, offset in [('selection', 'selected', None), ('caret', 'visible', 0), ('caret', 'visible', 26)]:
    pair = [next(c for c in base['cases'] if c['kind'] == kind and c['mode'] == 'ppc16'
                 and c['insertion_offset'] == offset and c['scale'] == 1 and c['state'] == state)
            for state in [before, 'resumed']]
    for phase in range(2):
        for image_kind in ['guest', 'rendered']:
            with tempfile.TemporaryDirectory(prefix='systemless-activation-pixels-') as temporary:
                out = pathlib.Path(temporary)
                manifest = copy.deepcopy(base)
                manifest.update(cases=copy.deepcopy(pair), complete=False)
                for case in manifest['cases']:
                    for key in ['guest', 'rendered', 'evidence']:
                        (out / case[key]).symlink_to((args.source / case[key]).resolve())
                (out / 'progress.json').write_text(json.dumps(manifest))
                assert len(verifier.verify(out, True)) == 1
                case = manifest['cases'][phase]
                path = out / case[image_kind]
                path.unlink()
                with Image.open(args.source / case[image_kind]) as image:
                    image = image.convert('RGBA')
                    factor = 1 if image_kind == 'guest' else 2
                    point = (80 * factor, 130 * factor)
                    r, g, b, a = image.getpixel(point)
                    image.putpixel(point, (r ^ 1, g, b, a))
                    image.save(path)
                case[image_kind + '_sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
                (out / 'progress.json').write_text(json.dumps(manifest))
                verifier.provenance.verify(out, True)
                try:
                    verifier.verify(out, True)
                except AssertionError as error:
                    assert 'restored field pixels differ' in str(error)
                else:
                    raise AssertionError('accepted altered restoration pixels')
                name = f'{kind}-{offset}-{case["state"]}-{image_kind}'
                rejected.append(name)
                print('Rejected ' + name)
report = {'verifier_sha256': hashlib.sha256(script.read_bytes()).hexdigest(),
          'rejected': rejected, 'scope': 'Restoration equality rejection checks with valid updated hashes; no native font oracle.'}
if args.report:
    args.report.write_text(json.dumps(report, indent=2) + '\n')
