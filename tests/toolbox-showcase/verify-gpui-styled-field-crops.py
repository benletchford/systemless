#!/usr/bin/env python3
"""Verify source membership and lossless pixels of reviewed field crops.

This checks the derivation of archived visual evidence, not font appearance.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify(directory):
    manifest = json.loads((directory / "progress.json").read_text())
    review = json.loads((directory / "field-crop-review.json").read_text())
    expected = {case["rendered"]: case["rendered_sha256"] for case in manifest["cases"]}
    seen = set()
    for sheet in review["sheets"]:
        path = directory / sheet["file"]
        assert digest(path) == sheet["sha256"], "review sheet changed"
        with Image.open(path) as image:
            pixels = image.convert("RGBA")
            for crop in sheet["crops"]:
                name = crop["source"]
                assert name in expected and name not in seen, "invalid or duplicate source"
                seen.add(name)
                source = directory / name
                assert digest(source) == expected[name] == crop["source_sha256"], "source changed"
                with Image.open(source) as original:
                    box = crop["source_box"]
                    target = crop["sheet_box"]
                    assert 0 <= box[0] < box[2] <= original.width
                    assert 0 <= box[1] < box[3] <= original.height
                    assert 0 <= target[0] < target[2] <= pixels.width
                    assert 0 <= target[1] < target[3] <= pixels.height
                    actual = original.convert("RGBA").crop(box)
                    derived = pixels.crop(target)
                    assert actual.size == derived.size
                    assert actual.tobytes() == derived.tobytes(), "crop pixels changed"
    assert seen == set(expected), "crop review does not cover the complete case set"
    return len(seen)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    args = parser.parse_args()
    print(f"Verified {verify(args.directory)} source-pinned, pixel-identical field crops; appearance remains an explicit human review")
