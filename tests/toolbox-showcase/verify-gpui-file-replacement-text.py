#!/usr/bin/env python3
"""Compare the replacement message with native guest paint, not a Macintosh oracle."""
import argparse
import json
import math
from pathlib import Path
from PIL import Image


def verify(path):
    evidence = json.loads(path.with_suffix('.json').read_text())
    assert evidence['compositor'] == 'shared Demo renderer'
    assert evidence['confirming_replace'] and evidence['name']
    assert evidence['actual_depth'] in {1, 8, 16}
    requested = evidence['requested_depth']
    expected = requested if requested is not None else 16 if evidence['prefer_powerpc'] else 8
    assert evidence['actual_depth'] == expected, 'actual framebuffer depth differs'
    assert evidence['scale'] in {0.75, 1, 1.5, 2}
    with Image.open(path) as image:
        rendered = image.convert('RGBA')
    with Image.open(path.with_suffix('.guest.png')) as image:
        guest = image.convert('RGBA')
    factor = rendered.width / guest.width
    assert rendered.height == round(guest.height * factor)
    density = factor / evidence['scale']
    assert density >= 1 and density == round(density)
    top, left, bottom, right = evidence['message_bounds']
    assert 0 <= top < bottom <= guest.height and 0 <= left < right <= guest.width
    snap = lambda value: math.ceil(value * factor - 0.5)
    source, actual = guest.load(), rendered.load()
    checked = 0
    ink = 0
    for y in range(top, bottom):
        for x in range(left, right):
            ink += source[x, y][:3] != (255, 255, 255)
            for dy in range(snap(y), snap(y + 1)):
                for dx in range(snap(x), snap(x + 1)):
                    assert actual[dx, dy] == source[x, y], (path, (dx, dy), actual[dx, dy], source[x, y])
                    checked += 1
    assert checked and ink, 'message must contain native ink'
    print(f'{path.name}: {checked} message pixels match guest paint; fixture scope only')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('captures', nargs='+', type=Path)
    for capture in parser.parse_args().captures:
        verify(capture)
