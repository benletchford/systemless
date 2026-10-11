#!/usr/bin/env python3
"""Verify the two-case native caret colour counterexample, not a full matrix."""
import argparse
import hashlib
import importlib.util
import json
import subprocess
from pathlib import Path
from PIL import Image


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    assert manifest['qualification_kind'] == 'caret_colour_counterexample'
    repo = Path(__file__).resolve().parents[2]
    sha = lambda data: hashlib.sha256(data).hexdigest()
    assert sha((repo / manifest['fixture']).read_bytes()) == manifest['fixture_sha256']
    for path, expected in manifest['source_sha256'].items():
        source = subprocess.check_output(['git', 'show', f"{manifest['source_commit']}:{path}"], cwd=repo)
        assert sha(source) == expected, f'pinned source differs: {path}'
    spec = importlib.util.spec_from_file_location('styled_ink', Path(__file__).with_name('verify-gpui-styled-text-ink.py'))
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    cases = manifest['cases']
    assert len(cases) == 2 and {case['mode'] for case in cases} == {'colour', 'ppc8'}
    colours = {}
    for case in cases:
        assert case['scale'] == 1 and case['device_scale'] == 2 and case['insertion_offset'] == 0
        assert case['selection'] == [0, 0] and case['active'] and case['caret_visible']
        verifier.verify_case(args.manifest.parent, case, manifest['field_bounds'])
        with Image.open(args.manifest.parent / case['guest']) as image:
            guest = image.convert('RGB')
        x, top, bottom = case['caret_column']
        colour = tuple(case['caret_rgb'])
        assert top < bottom and all(guest.getpixel((x, y)) == colour for y in range(top, bottom))
        colours[case['mode']] = colour
    assert colours['colour'] != colours['ppc8'], 'the CPU paint-policy counterexample disappeared'
    print('verified two styled caret colour counterexample captures; no full-matrix qualification')


if __name__ == '__main__':
    main()
