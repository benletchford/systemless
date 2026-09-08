#!/usr/bin/env python3
"""Verify that every fixture page has current, traceable native coverage."""
import json
from pathlib import Path
import re
import verify_captures


def main():
    fixture = Path(__file__).resolve().parents[1]
    coverage = json.loads((fixture / 'oracle/coverage.json').read_text())
    assert set(coverage['profiles']) == {'systemless-classic-68k', 'systemless-classic-ppc'}
    assert set(coverage['native_emulators']) == {'basilisk', 'sheepshaver'}
    source = (fixture / 'showcase.r').read_text()
    menu = source.split("resource 'MENU' (mPages, preload)", 1)[1].split('};', 1)[0]
    page_names = re.findall(r'^\s*"([^"]+)", noIcon,', menu, re.MULTILINE)
    pages = coverage['pages']
    assert [page['name'] for page in pages] == page_names, 'coverage must match the actual Pages menu'
    assert [page['page'] for page in pages] == list(range(1, len(page_names) + 1))
    verify_captures.main()
    manifests = {path.stem.removesuffix('-capture'): json.loads(path.read_text())
                 for path in (fixture / 'oracle').glob('*-capture.json')}
    expected_backends = set(coverage['native_emulators'])
    for name, manifest in manifests.items():
        assert {run['emulator'] for run in manifest['runs']} == expected_backends, name
        actions = json.loads((fixture / manifest['scenario']).read_text())['actions']
        names = {action['path'] for action in actions if action['type'] == 'screenshot'}
        for run in manifest['runs']:
            captured = {Path(check['file']).name for check in run['checkpoints'] if check['file'].endswith('.png')}
            assert captured == names, f'{name}: incomplete {run["emulator"]} capture inventory'
    assert {name for page in pages for name in page['native_scenarios']} == manifests.keys()
    overview = manifests['overview']
    for page in pages:
        assert page['contract'].strip()
        assert set(page['native_scenarios']) <= manifests.keys(), page['name']
        for run in overview['runs']:
            assert page['overview_checkpoint'] in {Path(check['file']).name for check in run['checkpoints']}, page['name']
        for profile in coverage['profiles']:
            for checkpoint in page['systemless_checkpoints']:
                assert (fixture / 'reference' / profile / checkpoint).is_file(), f'{profile}: {checkpoint}'
    print(f'{len(pages)} pages, {len(manifests)} native scenarios, both emulators and both classic Systemless profiles have current evidence')
    print('This verifies coverage and artifact identities; fresh native behavior requires the replay outcome verifiers.')


if __name__ == '__main__':
    main()
