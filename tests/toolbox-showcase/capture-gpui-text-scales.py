#!/usr/bin/env python3
"""Capture fresh GPUI document or modal TextEdit scale evidence on macOS.

Build first: cargo build --locked --example gpui-menu-demo --features gpui-demo-test
Run with an empty output directory; this never replaces reference captures.
Use --surface modal for nickname selection (offsets 3..6) and field focus changes.
Modal host-suspended states are not yet included.
Uses only the Python standard library. Visual review remains required.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess


def dimensions(path):
    header = path.read_bytes()[:24]
    if header[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError(f'not a PNG: {path}')
    return struct.unpack('>II', header[16:24])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--surface', choices=['document', 'modal'], default='document',
                        help='guest-owned text surface to capture')
    parser.add_argument('--state', choices=['all', 'active', 'inactive', 'host-suspended'],
                        default='all', help='capture only one supported state')
    arguments = parser.parse_args()
    output = arguments.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()):
        parser.error('output directory must be empty')
    repository = Path(__file__).resolve().parents[2]
    binary = repository / 'target/debug/examples/gpui-menu-demo'
    fixture = repository / 'tests/toolbox-showcase/toolbox-showcase.sit'
    records = []
    density = None
    states = ([('active', '--capture-text-edit-selected'),
               ('inactive', '--capture-text-edit-inactive'),
               ('host-suspended', '--capture-text-edit-host-suspended')]
              if arguments.surface == 'document' else
              [('active', '--capture-modal-dialog-selection'),
               ('inactive', '--capture-modal-dialog-selection-inactive')])
    if arguments.state != 'all':
        states = [(state, flag) for state, flag in states if state == arguments.state]
        if not states:
            parser.error('the selected surface does not support this state')
    prefix = 'text' if arguments.surface == 'document' else 'dialog-modal-selection'
    for mode, options in [('mono', ['--screen-depth', '1']),
                          ('colour', ['--screen-depth', '8']),
                          ('ppc', ['--prefer-powerpc'])]:
        for state, flag in states:
            for scale, label in [(0.75, '075'), (1., '100'), (1.5, '150'), (2., '200')]:
                image = output / f'{prefix}-scale-{mode}-{state}-{label}.png'
                command = [str(binary), str(fixture), *options,
                           '--capture-scale', str(scale), flag, str(image)]
                with image.with_suffix('.log').open('w') as log:
                    subprocess.run(command, cwd=repository, stdout=log,
                                   stderr=subprocess.STDOUT, check=True)
                guest = image.with_suffix('.guest.png')
                width, height = dimensions(image)
                guest_width, guest_height = dimensions(guest)
                current_density = width / (guest_width * scale)
                if density is None:
                    density = current_density
                if abs(current_density - density) > 1e-6 or width * guest_height != height * guest_width:
                    raise ValueError(f'scene scale/aspect changed: {image}')
                records.append({'mode': mode, 'state': state, 'scale': scale,
                                'command': command, 'host_pixel_density': current_density,
                                'artifacts': [{'file': path.name,
                                               'dimensions': dimensions(path),
                                               'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
                                              for path in (image, guest)]})
                print(image.name, flush=True)
    manifest = {'surface': arguments.surface, 'fixture_sha256': hashlib.sha256(fixture.read_bytes()).hexdigest(),
                'scope': 'Shared GPUI compositor and guest session API; not native host observer or native Macintosh qualification.',
                'visual_review': 'pending', 'captures': records}
    (output / f'{prefix}-scale{"" if arguments.state == "all" else "-" + arguments.state}-review.json').write_text(json.dumps(manifest, indent=2) + '\n')


if __name__ == '__main__':
    main()
