#!/usr/bin/env python3
"""Verify complete, hashed GPUI standard-list fixture capture matrices."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(path):
    manifest = json.loads(path.read_text())
    assert manifest['complete'], 'matrix is incomplete'
    assert len(manifest['capture_binary_sha256']) == 64
    assert len(manifest['fixture_sha256']) == 64
    assert manifest['source_sha256'], 'missing source provenance'
    assert all(len(value) == 64 for value in manifest['source_sha256'].values())
    states = ['scrolled', 'inactive', 'reactivated'] if any(
        'state' in case for case in manifest['cases']) else ['selected']
    states = manifest.get('states', states)
    assert states in [['selected'], ['scrolled', 'inactive', 'reactivated'], ['mutated', 'resized']]
    depths = {'mono': 1, 'colour': 8, 'ppc8': 8, 'ppc16': 16}
    required = {(mode, state, scale) for mode in depths for state in states
                for scale in [0.75, 1, 1.5, 2]}
    verifier_path = Path(__file__).with_name('verify-gpui-list-text.py')
    spec = importlib.util.spec_from_file_location('list_pixels', verifier_path)
    pixels = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(pixels)
    seen = set()
    for case in manifest['cases']:
        key = (case['mode'], case.get('state', 'selected'), case['scale'])
        assert key in required and key not in seen, ('unexpected or duplicate case', key)
        seen.add(key)
        assert case['depth'] == depths[case['mode']]
        rendered = path.parent / case['rendered']
        for extension, field in [('.png', 'rendered_sha256'),
                                 ('.guest.png', 'guest_sha256'),
                                 ('.json', 'evidence_sha256')]:
            assert digest(rendered.with_suffix(extension)) == case[field], field
        evidence = json.loads(rendered.with_suffix('.json').read_text())
        assert evidence['paint_depths'] == [case['depth']]
        assert evidence['prefer_powerpc'] == case['mode'].startswith('ppc')
        assert evidence['scale'] == case['scale']
        if key[1] != 'selected':
            assert evidence['transition'] == key[1]
        pixels.verify(rendered)
    assert seen == required, ('missing cases', required - seen)
    print(f'verified {len(seen)} list fixture cases; scope: {manifest["scope"]}')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    verify(parser.parse_args().manifest)
