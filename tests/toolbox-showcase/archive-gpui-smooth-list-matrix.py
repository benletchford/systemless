#!/usr/bin/env python3
"""Archive a complete smooth list capture matrix without changing decoded RGBA.

Validates provenance before and after compression. Visual review remains explicit;
this script never promotes retained-source appearance into raster qualification.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
from PIL import Image


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def archive(manifest_path, output):
    verifier = Path(__file__).with_name('verify-gpui-smooth-list-provenance.py')
    subprocess.run([sys.executable, str(verifier), str(manifest_path)], check=True)
    data = json.loads(manifest_path.read_text())
    output.mkdir(parents=True, exist_ok=False)
    for case in data['cases']:
        name = Path(case['rendered'])
        originals = {}
        for suffix, key in [('.png', 'rendered_sha256'), ('.guest.png', 'guest_sha256')]:
            source = (manifest_path.parent / name).with_suffix(suffix)
            target = (output / name).with_suffix(suffix)
            assert digest(source) == case[key], 'source changed during archive'
            originals[key] = case[key]
            with Image.open(source) as image:
                rgba = image.convert('RGBA')
                rgba.save(target, optimize=True, compress_level=9)
                with Image.open(target) as compressed:
                    assert compressed.convert('RGBA').tobytes() == rgba.tobytes(), 'decoded pixels changed'
            case[key] = digest(target)
        case['original_png_sha256'] = originals
        for suffix, key in [('.json', 'evidence_sha256'), ('.capture.json', 'compositor_evidence_sha256')]:
            source = (manifest_path.parent / name).with_suffix(suffix)
            assert digest(source) == case[key], 'evidence changed during archive'
            shutil.copyfile(source, (output / name).with_suffix(suffix))
    review_path = manifest_path.parent / 'visual-review.json'
    if review_path.exists():
        review = json.loads(review_path.read_text())
        assert review['source_commit'] == data['source_commit']
        cases = {case['rendered']: case for case in data['cases']}
        for sample in review['cases']:
            case = cases[sample['rendered']]
            assert sample['rendered_sha256'] == case['original_png_sha256']['rendered_sha256']
            sample['archived_rendered_sha256'] = case['rendered_sha256']
        (output / 'review.json').write_text(json.dumps(review, indent=2) + '\n')
    data['archive'] = 'PNG compression only; decoded RGBA unchanged; visual review is explicitly scoped'
    archived = output / 'progress.json'
    archived.write_text(json.dumps(data, indent=2) + '\n')
    subprocess.run([sys.executable, str(verifier), str(archived)], check=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('output', type=Path, help='fresh archive directory')
    args = parser.parse_args()
    archive(args.manifest.resolve(), args.output.resolve())
