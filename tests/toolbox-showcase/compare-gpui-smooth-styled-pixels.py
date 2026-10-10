#!/usr/bin/env python3
"""Compare two qualified smooth capture matrices; this is equivalence, not a font oracle."""
import argparse, hashlib, importlib.util, json, pathlib, sys, tempfile
from PIL import Image
sys.dont_write_bytecode = True
checker = pathlib.Path(__file__).with_name('verify-gpui-smooth-styled-provenance.py')
spec = importlib.util.spec_from_file_location('provenance', checker)
provenance = importlib.util.module_from_spec(spec)
spec.loader.exec_module(provenance)

def compare(before, after, partial_after=False):
    if not partial_after:
        return _compare(before, after, False)
    # Freeze membership while the capture job appends new cases. Captured image
    # files are immutable; only progress.json is replaced by the live producer.
    after = pathlib.Path(after).resolve()
    progress = (after / 'progress.json').read_bytes()
    manifest = json.loads(progress)
    with tempfile.TemporaryDirectory(prefix='systemless-pixel-comparison-') as temporary:
        snapshot = pathlib.Path(temporary)
        (snapshot / 'progress.json').write_bytes(progress)
        for case in manifest['cases']:
            for kind in ['guest', 'rendered', 'evidence']:
                name = case[kind]
                assert pathlib.Path(name).name == name, 'capture paths must be basenames'
                (snapshot / name).symlink_to(after / name)
        return _compare(before, snapshot, True)


def _compare(before, after, partial_after):
    before, after = pathlib.Path(before), pathlib.Path(after)
    before_count = provenance.verify(before)
    count = provenance.verify(after, partial_after)
    assert count <= before_count, 'after matrix is larger'
    if not partial_after:
        assert count == before_count, 'matrix sizes differ'
    manifests = [json.loads((p / 'progress.json').read_text()) for p in [before, after]]
    assert manifests[0]['fixture_sha256'] == manifests[1]['fixture_sha256'], 'guest fixture differs'
    fields = ['kind', 'mode', 'state', 'insertion_offset', 'scale']
    indexed = [{tuple(case[k] for k in fields): case for case in m['cases']} for m in manifests]
    assert indexed[1].keys() <= indexed[0].keys(), 'after cases absent from baseline'
    if not partial_after:
        assert indexed[0].keys() == indexed[1].keys(), 'case membership differs'
    comparisons = []
    for key, updated in indexed[1].items():
        original = indexed[0][key]
        for kind in ['guest', 'rendered']:
            paths = [before / original[kind], after / updated[kind]]
            with Image.open(paths[0]) as a, Image.open(paths[1]) as b:
                assert a.size == b.size, (key, kind, 'dimensions differ')
                pixels = a.convert('RGBA').tobytes()
                assert pixels == b.convert('RGBA').tobytes(), (key, kind, 'RGBA differs')
            comparisons.append({'case': list(key), 'kind': kind,
                                'rgba_sha256': hashlib.sha256(pixels).hexdigest()})
    return {'outcome': f'All {count * 2} guest/composed images match exact decoded RGBA',
            'after_complete': manifests[1]['complete'],
            'partial_after_requested': partial_after,
            'before_source': manifests[0]['source_commit'],
            'after_source': manifests[1]['source_commit'],
            'scope': 'Complete baseline and complete or explicitly partial after-matrix provenance/state/geometry verified independently before comparison. Partial comparison qualifies only captured cases. Equivalence between Systemless runs does not establish original Macintosh font appearance, physical host interaction or performance.',
            'images': comparisons}

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=pathlib.Path)
    parser.add_argument('after', type=pathlib.Path)
    parser.add_argument('--report', type=pathlib.Path)
    parser.add_argument('--partial-after', action='store_true', help='compare only currently captured after cases; complete baseline remains required')
    args = parser.parse_args()
    report = compare(args.before, args.after, args.partial_after)
    if args.report:
        args.report.write_text(json.dumps(report, indent=2) + '\n')
    print(report['outcome'])
