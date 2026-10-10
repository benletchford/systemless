#!/usr/bin/env python3
"""Compare two qualified smooth capture matrices; this is equivalence, not a font oracle."""
import argparse, hashlib, importlib.util, json, pathlib, sys
from PIL import Image
sys.dont_write_bytecode = True
checker = pathlib.Path(__file__).with_name('verify-gpui-smooth-styled-provenance.py')
spec = importlib.util.spec_from_file_location('provenance', checker)
provenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provenance)

def compare(before, after):
    before, after = pathlib.Path(before), pathlib.Path(after)
    count = provenance.verify(before)
    assert provenance.verify(after) == count, 'matrix sizes differ'
    manifests = [json.loads((p / 'progress.json').read_text()) for p in [before, after]]
    assert manifests[0]['fixture_sha256'] == manifests[1]['fixture_sha256'], 'guest fixture differs'
    fields = ['kind', 'mode', 'state', 'insertion_offset', 'scale']
    indexed = [{tuple(case[k] for k in fields): case for case in m['cases']} for m in manifests]
    assert indexed[0].keys() == indexed[1].keys(), 'case membership differs'
    comparisons = []
    for key, original in indexed[0].items():
        updated = indexed[1][key]
        for kind in ['guest', 'rendered']:
            paths = [before / original[kind], after / updated[kind]]
            with Image.open(paths[0]) as a, Image.open(paths[1]) as b:
                assert a.size == b.size, (key, kind, 'dimensions differ')
                pixels = a.convert('RGBA').tobytes()
                assert pixels == b.convert('RGBA').tobytes(), (key, kind, 'RGBA differs')
            comparisons.append({'case': list(key), 'kind': kind,
                                'rgba_sha256': hashlib.sha256(pixels).hexdigest()})
    return {'outcome': f'All {count * 2} guest/composed images match exact decoded RGBA',
            'before_source': manifests[0]['source_commit'],
            'after_source': manifests[1]['source_commit'],
            'scope': 'Full matrix provenance/state/geometry verified independently before comparison. Equivalence between Systemless runs does not establish original Macintosh font appearance, physical host interaction or performance.',
            'images': comparisons}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=pathlib.Path)
    parser.add_argument('after', type=pathlib.Path)
    parser.add_argument('--report', type=pathlib.Path)
    args = parser.parse_args()
    report = compare(args.before, args.after)
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(report['outcome'])
