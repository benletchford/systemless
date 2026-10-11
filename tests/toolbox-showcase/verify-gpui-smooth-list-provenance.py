#!/usr/bin/env python3
"""Check smooth selected-list capture provenance and geometry, not raster parity.

Retained guest pixels can conceal missing replacement paint. A passing result
requires visual review and does not qualify font fidelity or release readiness.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def dimensions(path):
    header = path.read_bytes()[:24]
    assert header[:8] == b'\x89PNG\r\n\x1a\n' and header[12:16] == b'IHDR'
    return list(struct.unpack('>II', header[16:24]))


def verify_case(directory, case):
    name = Path(case['rendered'])
    assert name.name == str(name), 'capture must be local to the manifest'
    assert case['depth'] == {'mono': 1, 'colour': 8, 'ppc8': 8, 'ppc16': 16}[case['mode']]
    image = directory / name
    for path, key in [(image, 'rendered_sha256'),
                      (image.with_suffix('.guest.png'), 'guest_sha256'),
                      (image.with_suffix('.json'), 'evidence_sha256'),
                      (image.with_suffix('.capture.json'), 'compositor_evidence_sha256')]:
        assert digest(path) == case[key], f'capture changed: {path.name}'
    evidence = json.loads(image.with_suffix('.json').read_text())
    capture = json.loads(image.with_suffix('.capture.json').read_text())
    assert evidence['source_mask'] == 'retained native source; appearance only'
    assert evidence['erased_regions'] is None
    assert capture['case'] == 'ListsSelectedNativeSource'
    assert capture['compositor'] == evidence['compositor'] == 'shared Demo renderer'
    assert capture['actual_depth'] == case['depth']
    assert evidence['paint_depths'] == [case['depth']]
    assert capture['prefer_powerpc'] == evidence['prefer_powerpc'] == case['mode'].startswith('ppc')
    assert capture['requested_scale'] == capture['scene_scale'] == evidence['scale'] == case['scale']
    assert capture['scene_origin'] == [0, 0]
    guest = dimensions(image.with_suffix('.guest.png'))
    composed = dimensions(image)
    assert guest == capture['guest_dimensions'] == [800, 600]
    assert composed == capture['composed_dimensions']
    assert capture['viewport_dimensions'] == [v * case['scale'] for v in guest]
    density = composed[0] / capture['viewport_dimensions'][0]
    assert density >= 1 and density == round(density)
    assert composed[1] == capture['viewport_dimensions'][1] * density
    states = evidence['list_state']
    assert len(states) == 1
    state = states[0]
    assert state['id'] > 0 and state['generation'] > 0
    assert state['active'] and state['selected'] == [[7, 0]]
    assert state['visible'] == [0, 0, 9, 1]
    regions = evidence['owned_regions']
    assert regions and {tuple(cell) for _, cell, _ in regions} == {(row, 0) for row in range(9)}
    for owner, cell, (top, left, bottom, right) in regions:
        assert owner == 0 and 0 <= top < bottom <= guest[1] and 0 <= left < right <= guest[0]
    return case['mode'], case['scale']


def verify(path, partial=False):
    manifest = json.loads(path.read_text())
    assert manifest['source_dirty'] is False
    assert manifest['presentation'] == 'smooth retained-source appearance review'
    assert manifest['pixel_qualification'] is False
    assert manifest['states'] == ['selected']
    expected = {(mode, scale) for mode in ['mono', 'colour', 'ppc8', 'ppc16']
                for scale in [0.75, 1, 1.5, 2]}
    observed = [verify_case(path.parent, case) for case in manifest['cases']]
    assert observed and len(observed) == len(set(observed)) and set(observed) <= expected
    if not partial:
        assert manifest['complete'] and set(observed) == expected, 'matrix incomplete'
    print(f'{len(observed)}/16 captures have intact provenance, depth, geometry and guest selection; raster appearance unqualified')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('--partial', action='store_true', help='check only completed cases during a live capture')
    args = parser.parse_args()
    verify(args.manifest, args.partial)
