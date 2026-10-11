#!/usr/bin/env python3
"""Verify GPUI list ownership pixels after native source pixels were erased.

This checks retained standard-cell regions only. It does not qualify surrounding
chrome, custom drawing, interaction, lifecycle, or release readiness.
"""
import argparse
import json
import math
from pathlib import Path
from PIL import Image


def verify(path):
    evidence = json.loads(path.with_suffix('.json').read_text())
    assert evidence['compositor'] == 'shared Demo renderer'
    assert evidence['source_mask'] == 'magenta qualified visible regions'
    expected_depth = evidence['requested_depth']
    if expected_depth is None:
        expected_depth = 16 if evidence['prefer_powerpc'] else 8
    assert evidence['paint_depths'] == [expected_depth], 'actual paint depth differs'
    transition = evidence.get('transition', 'none')
    assert transition in {'none', 'scrolled', 'inactive', 'reactivated', 'mutated', 'resized'}
    if transition != 'none':
        states = evidence['list_state']
        assert len(states) == 1, 'transition fixture must retain one list'
        state = states[0]
        assert state['generation'] > 0 and state['id'] > 0
        assert state['selected'] == [[7, 0]], 'transition lost guest selection'
        assert state['active'] == (transition != 'inactive')
        assert state['visible'][0] == (4 if transition == 'scrolled' else 0)
        if transition == 'mutated':
            row = next(cell for cell in state['cells'] if cell['cell'] == [7, 0])
            assert bytes(row['bytes']).endswith(b'  * updated'), 'guest row did not mutate'
        if transition == 'resized':
            assert state['view_rect'] == [78, 24, 192, 474], 'guest list did not resize'
    regions = evidence['erased_regions']
    assert regions, 'capture has no GPUI ownership'
    with Image.open(path) as image:
        rendered = image.convert('RGBA')
    with Image.open(path.with_suffix('.guest.png')) as image:
        guest = image.convert('RGBA')
    factor = rendered.width / guest.width
    assert rendered.height == round(guest.height * factor)
    scale = evidence['scale']
    assert scale is not None, 'use explicit capture scale for scene pixel verification'
    density = factor / scale
    assert density >= 1 and density == round(density), 'unexpected device scale'
    snap = lambda value: math.ceil(value * factor - 0.5)
    source, actual = guest.load(), rendered.load()
    checked = 0
    for _, _, (top, left, bottom, right) in regions:
        assert 0 <= top < bottom <= guest.height
        assert 0 <= left < right <= guest.width
        for y in range(top, bottom):
            for x in range(left, right):
                for dy in range(snap(y), snap(y + 1)):
                    for dx in range(snap(x), snap(x + 1)):
                        assert actual[dx, dy] == source[x, y], (
                            str(path), (dx, dy), actual[dx, dy], source[x, y])
                        checked += 1
    assert checked, 'no device pixels checked'
    print(f'{path.name}: {checked} owned device pixels match guest paint')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('captures', nargs='+', type=Path)
    for capture in parser.parse_args().captures:
        verify(capture)
