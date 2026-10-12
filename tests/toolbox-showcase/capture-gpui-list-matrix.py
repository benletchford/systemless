#!/usr/bin/env python3
"""Capture standard list transitions in the shared macOS GPUI compositor.

Build gpui-menu-demo with gpui-demo-test first. Use a fresh output directory;
the binary and source must remain unchanged while this script runs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--selected', action='store_true', help='capture selected state instead of transitions')
    parser.add_argument('--lifecycle', action='store_true', help='capture guest row mutation and resizing')
    parser.add_argument('--smooth-review', action='store_true', help='retain native source and collect smooth appearance evidence without binary pixel qualification')
    args = parser.parse_args()
    assert not (args.selected and args.lifecycle), 'choose one matrix'
    root = Path(__file__).resolve().parents[2]
    assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=root), 'commit source before capturing'
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    binary = root / 'target/debug/examples/gpui-menu-demo'
    fixture = root / 'tests/toolbox-showcase/toolbox-showcase.sit'
    paths = ['src/bin/gpui_demo.rs', 'src/bin/gpui_demo_text.rs', 'src/bin/gpui_demo_frames.rs',
             'src/systems/macintosh/list_manager.rs', 'src/systems/macintosh/text_edit/drawing.rs',
             'src/systems/macintosh/runner/mod.rs', 'src/systems/macintosh/trap/toolbox.rs',
             'src/systems/macintosh/loader/ppc/dispatch_list.rs',
             'tests/toolbox-showcase/verify-gpui-list-text.py',
             'tests/toolbox-showcase/verify-gpui-list-matrix.py',
             'tests/toolbox-showcase/capture-gpui-list-matrix.py']
    manifest = {
        'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
        'source_dirty': False, 'source_sha256': {p: digest(root / p) for p in paths},
        'capture_binary_sha256': digest(binary), 'fixture_sha256': digest(fixture),
        'cases': [], 'complete': False,
        'presentation': 'smooth retained-source appearance review' if args.smooth_review else 'binary erased-source qualification',
        'pixel_qualification': not args.smooth_review,
        'scope': 'Standard list fixture owned paint and guest-button states; no general lifecycle, GPUI pointer or native host observer qualification',
    }
    progress = output / 'progress.json'
    def save():
        progress.write_text(json.dumps(manifest, indent=2) + '\n')
    save()
    states = ['mutated', 'resized'] if args.lifecycle else ['selected'] if args.selected else ['scrolled', 'inactive', 'reactivated']
    manifest['states'] = states
    for mode, depth, ppc in [('mono', 1, False), ('colour', 8, False), ('ppc8', 8, True), ('ppc16', 16, True)]:
        for state in states:
            for scale in [0.75, 1, 1.5, 2]:
                assert digest(binary) == manifest['capture_binary_sha256'], 'binary changed during capture'
                assert digest(fixture) == manifest['fixture_sha256']
                assert all(digest(root / p) == value for p, value in manifest['source_sha256'].items())
                rendered = output / f'{mode}-{state}-{scale}.png'
                flag = '--capture-lists-selected' if state == 'selected' else '--capture-lists-transition'
                command = [str(binary), str(fixture), flag, str(rendered), '--capture-scale', str(scale)]
                if args.smooth_review: command += ['--capture-lists-retain-native-source']
                if state != 'selected': command += ['--capture-list-transition', state]
                if depth != 16: command += ['--screen-depth', str(depth)]
                if ppc: command += ['--prefer-powerpc']
                with rendered.with_suffix('.log').open('w') as log:
                    subprocess.run(command, cwd=root, stdout=log, stderr=log, check=True)
                if not args.smooth_review:
                    subprocess.run(['python3', str(root / 'tests/toolbox-showcase/verify-gpui-list-text.py'), str(rendered)], check=True)
                capture = rendered.with_suffix('.capture.json')
                assert capture.is_file(), 'missing shared compositor provenance'
                manifest['cases'].append({
                    'mode': mode, 'depth': depth, 'scale': scale,
                    **({'state': state} if state != 'selected' else {}),
                    'command': command, 'rendered': rendered.name,
                    'rendered_sha256': digest(rendered),
                    'guest_sha256': digest(rendered.with_suffix('.guest.png')),
                    'evidence_sha256': digest(rendered.with_suffix('.json')),
                    'compositor_evidence_sha256': digest(capture),
                })
                save()
    manifest['complete'] = True
    save()
    if args.smooth_review:
        subprocess.run(['python3', str(root / 'tests/toolbox-showcase/verify-gpui-smooth-list-provenance.py'), str(progress)], check=True)
    else:
        subprocess.run(['python3', str(root / 'tests/toolbox-showcase/verify-gpui-list-matrix.py'), str(progress)], check=True)


if __name__ == '__main__':
    main()
