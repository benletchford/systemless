#!/usr/bin/env python3
"""Verify the 192-case fixture matrix; this does not prove production readiness."""
import argparse
import hashlib
import importlib.util
import json
import subprocess
from pathlib import Path

MODES = {'mono': 1, 'colour': 8, 'ppc8': 8, 'ppc16': 16}
SCALES = [0.75, 1, 1.5, 2]
CARET_STATES = ['visible', 'blink-off', 'suspended', 'resumed']


def required_cases(matrix_kind="single-line"):
    assert matrix_kind in {"single-line", "multiline"}, "unknown matrix kind"
    if matrix_kind == "multiline":
        return {("multiline", mode, scale, "selected", None) for mode in MODES for scale in SCALES}
    configs = [('inactive', 'inactive', None)]
    configs += [('selection', state, None) for state in ['selected', 'suspended', 'resumed']]
    configs += [('caret', state, offset) for offset in [0, 26] for state in CARET_STATES]
    return {(kind, mode, scale, state, offset) for mode in MODES
            for kind, state, offset in configs for scale in SCALES}


def verify_manifest(path, partial=False):
    manifest = json.loads(path.read_text())
    assert not manifest.get('qualification_invalid_reason'), manifest.get('qualification_invalid_reason')
    if not partial:
        assert manifest['complete'], 'capture job has not completed'
        assert manifest['source_sha256'], 'missing pinned source hashes'
    repo = Path(__file__).resolve().parents[2]
    sha = lambda data: hashlib.sha256(data).hexdigest()
    assert sha((repo / manifest['fixture']).read_bytes()) == manifest['fixture_sha256']
    for source_path, expected in manifest.get('source_sha256', {}).items():
        source = subprocess.check_output(['git', 'show', f"{manifest['source_commit']}:{source_path}"], cwd=repo)
        assert sha(source) == expected, f'pinned source differs: {source_path}'
    assert len(manifest['capture_binary_sha256']) == 64
    spec = importlib.util.spec_from_file_location('styled_ink', Path(__file__).with_name('verify-gpui-styled-text-ink.py'))
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    required = required_cases(manifest.get("matrix_kind", "single-line"))
    seen = set()
    for case in manifest['cases']:
        key = (case['kind'], case['mode'], case['scale'], case['state'], case['insertion_offset'])
        assert key in required and key not in seen, f'unknown/duplicate case: {key}'
        seen.add(key)
        assert case['actual_depth'] == MODES[case['mode']] and case['device_scale'] == 2
        for field in ['rendered', 'guest', 'evidence']:
            assert sha((path.parent / case[field]).read_bytes()) == case[field + '_sha256'], field + ' hash differs'
        evidence = json.loads((path.parent / case['evidence']).read_text())
        assert evidence['depth'] == case['actual_depth'] and evidence['scale'] == case['scale']
        assert evidence['drawing_intact'] and evidence['generation'] > 0
        assert evidence['view'] == manifest['field_bounds']
        assert evidence['active'] == (case['state'] not in ['inactive', 'suspended'])
        offset = case['insertion_offset']
        expected = [0, 31] if case['kind'] == 'multiline' else [0, 26] if case['kind'] == 'selection' else [offset or 0, offset or 0]
        assert bool(evidence.get('multiline', False)) == (case['kind'] == 'multiline')
        if manifest.get('compositor'):
            assert evidence.get('compositor') == manifest['compositor'], 'wrong compositor'
        assert evidence['selection'] == expected, f'wrong guest range: {key}'
        if case['kind'] == 'caret':
            assert evidence['caret_state'] == case['state'] and evidence['insertion_offset'] == offset
            if case['state'] != 'suspended':
                assert evidence['caret_visible'] == (case['state'] != 'blink-off')
        else:
            assert evidence['caret_state'] == 'not-requested' and offset is None
        verifier.verify_case(path.parent, case, manifest['field_bounds'])
    if not partial:
        assert seen == required, 'incomplete CPU/depth/scale/state/insertion matrix'
    print(f'verified {len(seen)}/{len(required)} styled field captures; ' +
          ('incomplete qualification' if partial else 'fixture raster matrix only, not production readiness'))
    return manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('--partial', action='store_true', help='check completed cases without qualifying the full matrix')
    args = parser.parse_args()
    verify_manifest(args.manifest, args.partial)


if __name__ == '__main__':
    main()
