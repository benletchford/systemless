#!/usr/bin/env python3
"""Check a fresh full-page native replay against reviewed outcome regions."""
import argparse
import array
import hashlib
import json
from pathlib import Path
import sys
from PIL import Image, ImageChops, ImageDraw


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def region(path, box, masks):
    image = Image.open(path).convert('RGB')
    draw = ImageDraw.Draw(image)
    for left, top, right, bottom in masks:
        draw.rectangle((left, top, right - 1, bottom - 1), fill=(255, 255, 255))
    return image.crop(box)


def verify(fixture, directory, manifest, backend):
    output = directory / (backend + '_output')
    capture = json.loads((directory / (backend + '-capture.json')).read_text())
    assert capture['fixture_sha256'] == sha(fixture / 'toolbox-showcase.sit'), 'replay used a different fixture'
    assert capture['scenario_sha256'] == sha(fixture / manifest['scenario']), 'replay used a different scenario'
    run = next(run for run in manifest['runs'] if run['emulator'] == backend)
    assert capture['image_id'] == run['image_id'], 'review required for a different emulator image'
    checkpoints = run['checkpoints']
    expected_names = {Path(check['file']).name for check in checkpoints}
    scenario = json.loads((fixture / manifest['scenario']).read_text())
    names = {action['path'] for action in scenario['actions'] if action['type'] == 'screenshot'}
    assert expected_names == names, 'every replay checkpoint needs reviewed evidence'
    comparisons = 0
    for checkpoint in checkpoints:
        expected = fixture / checkpoint['file']
        assert sha(expected) == checkpoint['sha256'], 'reviewed evidence was modified'
        actual = output / expected.name
        masks = checkpoint.get('masks', [])
        for box in checkpoint['regions']:
            difference = ImageChops.difference(region(actual, box, masks), region(expected, box, masks))
            assert difference.getbbox() is None, f'{backend}: {actual.name}: outcome region {box} differs'
            comparisons += 1
    relations = []
    for check in manifest['relations']:
        if backend not in check.get('backends', ['basilisk', 'sheepshaver']):
            continue
        first = output / check['first']
        second = output / check['second']
        masks = [mask for checkpoint in checkpoints
                 if Path(checkpoint['file']).name in [first.name, second.name]
                 for mask in checkpoint.get('masks', [])]
        a = region(first, check.get('first_region', check.get('region')), masks)
        b = region(second, check.get('second_region', check.get('region')), masks)
        equal = ImageChops.difference(a, b).getbbox() is None
        assert equal == check['equal'], f"{backend}: {check['label']}"
        relations.append(check['label'])

    # The earlier About alert is outside this interval. The isolated SysBeep
    # action must generate audible samples, then return to silence before the
    # final screenshot. Device blocks are not exact raster timestamps.
    def frame(name):
        return json.loads((output / (name + '.ctx.json')).read_text())['audio']['frames_written']
    start = frame('overview-10-sound-before-beep')
    end = frame('overview-10-sound-beeped')
    metadata = json.loads((output / 'oracle-audio.json').read_text())
    assert metadata['sample_rate'] == 44100 and metadata['channels'] == 2
    assert metadata['format'] == 'signed 16-bit big-endian'
    raw = output / 'oracle-audio.raw'
    assert raw.stat().st_size == metadata['capture_bytes']
    assert 0 <= start < end <= metadata['frames_written']
    with raw.open('rb') as stream:
        stream.seek(start * 4)
        samples = array.array('h')
        samples.frombytes(stream.read((end - start) * 4))
    if sys.byteorder == 'little':
        samples.byteswap()
    non_silent = sum(left != 0 or right != 0 for left, right in zip(samples[::2], samples[1::2]))
    assert non_silent >= 256, 'SysBeep did not produce a sustained audible alert'
    assert not any(samples[-8192:]), 'SysBeep has not returned to silence'
    return {'emulator': backend, 'fixture_sha256': capture['fixture_sha256'],
            'scenario_sha256': capture['scenario_sha256'], 'image_id': capture['image_id'],
            'checkpoints': len(checkpoints), 'outcome_regions': comparisons,
            'relations': relations, 'sysbeep_non_silent_frames': non_silent,
            'sysbeep_interval': [start, end], 'source_pcm_sha256': sha(raw),
            'result': 'pass'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('capture_directory', type=Path, help='directory containing both emulator output folders and capture reports')
    args = parser.parse_args()
    fixture = Path(__file__).resolve().parents[1]
    manifest = json.loads((fixture / 'oracle/overview-capture.json').read_text())
    reports = [verify(fixture, args.capture_directory, manifest, backend) for backend in ['basilisk', 'sheepshaver']]
    (args.capture_directory / 'overview-verification.json').write_text(json.dumps(reports, indent=2) + '\n')
    for report in reports:
        print(f"{report['emulator']}: {report['checkpoints']} checkpoints, {report['outcome_regions']} reviewed regions, state transitions and SysBeep passed")


if __name__ == '__main__':
    main()
