#!/usr/bin/env python3
"""Losslessly archive a completed 192-case capture job with pinned source hashes."""
import argparse
import hashlib
import importlib.util
import json
import shutil
import subprocess
from pathlib import Path
from PIL import Image

SOURCES = [
    'src/bin/gpui_demo.rs', 'src/bin/gpui_demo_text.rs', 'src/bin/gpui_demo_frames.rs',
    'src/systems/macintosh/text_edit.rs', 'src/systems/macintosh/text_edit/drawing.rs',
    'src/systems/macintosh/memory/presentation.rs',
    'src/systems/macintosh/trap/dialog.rs', 'src/systems/macintosh/trap/quickdraw.rs',
    'src/systems/macintosh/loader/ppc/textedit.rs',
    'src/systems/macintosh/loader/ppc/dispatch_textedit.rs',
    'src/systems/macintosh/loader/ppc/loaded_app_display.rs',
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('progress', type=Path)
    parser.add_argument('destination', type=Path)
    args = parser.parse_args()
    manifest = json.loads(args.progress.read_text())
    if not manifest['complete']:
        parser.error('capture job is not complete; continue observing its existing handle')
    if args.destination.exists() and any(args.destination.iterdir()):
        parser.error('use a fresh archive directory')
    repo = Path(__file__).resolve().parents[2]
    sha = lambda data: hashlib.sha256(data).hexdigest()
    manifest['source_sha256'] = {
        path: sha(subprocess.check_output(['git', 'show', f"{manifest['source_commit']}:{path}"], cwd=repo))
        for path in SOURCES
    }
    spec = importlib.util.spec_from_file_location('matrix', Path(__file__).with_name('verify-gpui-styled-text-matrix.py'))
    verifier = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(verifier)
    # Validate original captures before accepting or recompressing their pixels.
    source_review = args.progress.with_name('archive-source-review.json')
    source_review.write_text(json.dumps(manifest, indent=2) + '\n')
    verifier.verify_manifest(source_review)
    args.destination.mkdir(parents=True, exist_ok=True)
    for case in manifest['cases']:
        for field in ['rendered', 'guest', 'evidence']:
            name = case[field]
            assert Path(name).name == name, 'archive filenames must be local basenames'
            original = args.progress.parent / name
            target = args.destination / name
            case['original_' + field + '_sha256'] = case[field + '_sha256']
            if field == 'evidence':
                shutil.copyfile(original, target)
            else:
                with Image.open(original) as image:
                    rgba = image.convert('RGBA')
                    rgba.save(target, optimize=True, compress_level=9)
                    with Image.open(target) as archived:
                        assert archived.convert('RGBA').tobytes() == rgba.tobytes(), 'recompression changed pixels'
            case[field + '_sha256'] = sha(target.read_bytes())
    manifest['archive_encoding'] = 'Lossless PNG recompression with RGBA equality; original byte hashes retained'
    manifest['platform'] = 'macOS; shared GPUI Metal headless compositor; device density 2'
    manifest['scope'] = ('Public mixed-style fixture field raster and guest state assertions only; '
                         'no production ownership, arbitrary themes/backgrounds/custom fonts, '
                         'GPUI pointer mapping, native host observer or Macintosh oracle qualification')
    review = args.destination / 'review.json'
    review.write_text(json.dumps(manifest, indent=2) + '\n')
    verifier.verify_manifest(review)
    print(f"Archived {len(manifest['cases'])} capture pairs and state sidecars without changing their pixels")


if __name__ == '__main__':
    main()
