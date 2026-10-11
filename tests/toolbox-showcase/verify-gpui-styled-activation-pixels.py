#!/usr/bin/env python3
"""Compare selected/resumed field pixels; not an original Macintosh font oracle."""
import argparse, importlib.util, json, math, pathlib, sys
from PIL import Image
sys.dont_write_bytecode = True
checker = pathlib.Path(__file__).with_name('verify-gpui-smooth-styled-provenance.py')
spec = importlib.util.spec_from_file_location('provenance', checker)
provenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provenance)

def verify(directory, partial=False):
    directory = pathlib.Path(directory)
    provenance.verify(directory, partial)
    manifest = json.loads((directory / 'progress.json').read_text())
    assert manifest['matrix_kind'] == 'single-line'
    cases = {(c['mode'], c['state'], c['scale']): c for c in manifest['cases'] if c['kind'] == 'selection'}
    pairs = []
    for mode in ['mono', 'colour', 'ppc8', 'ppc16']:
        for scale in [0.75, 1, 1.5, 2]:
            selected = cases.get((mode, 'selected', scale))
            resumed = cases.get((mode, 'resumed', scale))
            if selected is None or resumed is None:
                assert partial, (mode, scale, 'missing activation pair')
                continue
            top, left, bottom, right = manifest['field_bounds']
            for kind, factor in [('guest', 1), ('rendered', scale * selected['device_scale'])]:
                box = (math.floor(left * factor), math.floor(top * factor),
                       math.ceil(right * factor), math.ceil(bottom * factor))
                def pixels(case):
                    with Image.open(directory / case[kind]) as image:
                        return image.convert('RGBA').crop(box).tobytes()
                assert pixels(selected) == pixels(resumed), (mode, scale, kind, 'restored selection pixels differ')
            pairs.append({'mode': mode, 'scale': scale})
    assert pairs
    return pairs

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=pathlib.Path)
    parser.add_argument('--partial', action='store_true')
    parser.add_argument('--report', type=pathlib.Path)
    args = parser.parse_args()
    pairs = verify(args.directory, args.partial)
    report = {'pairs': pairs, 'partial': args.partial,
              'scope': 'Exact RGBA equality of guest and shared-compositor selected/resumed field crops. Does not establish original font appearance, host activation or performance.'}
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(f'{len(pairs)} selected/resumed guest and composed field pairs match exactly')
