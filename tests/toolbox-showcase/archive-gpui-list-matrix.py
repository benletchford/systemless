#!/usr/bin/env python3
"""Losslessly archive a complete and verified GPUI list capture matrix."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from PIL import Image


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('manifest', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    source = args.manifest.resolve()
    destination = args.destination.resolve()
    verifier = Path(__file__).with_name('verify-gpui-list-matrix.py')
    subprocess.run(['python3', str(verifier), str(source)], check=True)
    destination.mkdir(parents=True, exist_ok=False)
    manifest = json.loads(source.read_text())
    for case in manifest['cases']:
        original = source.parent / case['rendered']
        for extension, field in [('.png', 'rendered_sha256'),
                                 ('.guest.png', 'guest_sha256'),
                                 ('.json', 'evidence_sha256')]:
            path = original.with_suffix(extension)
            assert digest(path) == case[field]
            archived = destination / path.name
            if extension == '.json':
                archived.write_bytes(path.read_bytes())
            else:
                with Image.open(path) as image:
                    image.save(archived, compress_level=9)
                with Image.open(path) as a, Image.open(archived) as b:
                    assert a.size == b.size
                    assert a.convert('RGBA').tobytes() == b.convert('RGBA').tobytes()
            # Preserve the first original hash even when re-archiving.
            case.setdefault('original_' + field, case[field])
            case[field] = digest(archived)
        case['rendered'] = original.name
    manifest['original_output'] = str(source.parent)
    manifest['archived_losslessly'] = True
    review = destination / 'review.json'
    review.write_text(json.dumps(manifest, indent=2) + '\n')
    subprocess.run(['python3', str(verifier), str(review)], check=True)


if __name__ == '__main__':
    main()
