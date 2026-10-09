#!/usr/bin/env python3
"""Verify archived showcase popup ink against independently scaled guest pixels.

This checks selected labels and classic indicators, not full chrome or native
Macintosh typography. Requires Pillow; never regenerates reference images.
"""

import argparse
import hashlib
import json
import math
from pathlib import Path
import subprocess

from PIL import Image


def digest(data):
    return hashlib.sha256(data).hexdigest()


def verify(manifest_path):
    manifest = json.loads(manifest_path.read_text())
    root = manifest_path.parent
    repo = Path(__file__).resolve().parents[2]
    captures = manifest["captures"]
    expected_cases = {
        (mode, state, scale)
        for mode in ("mono", "colour", "ppc")
        for state in ("active", "host-suspended", "disabled")
        for scale in (0.75, 1.0, 1.5, 2.0)
    }
    assert len(captures) == 36
    assert {(c["mode"], c["state"], c["scale"]) for c in captures} == expected_cases
    for path, fingerprint in manifest["source_sha256"].items():
        data = subprocess.check_output(
            ["git", "show", f'{manifest["source_commit"]}:{path}'], cwd=repo
        )
        assert digest(data) == fingerprint, path

    regions_checked = 0
    for capture in captures:
        assert capture["host_pixel_density"] == 2.0
        assert len(capture["artifacts"]) == 2
        for artifact in capture["artifacts"]:
            path = root / artifact["file"]
            assert digest(path.read_bytes()) == artifact["sha256"], path
            with Image.open(path) as image:
                assert list(image.size) == artifact["dimensions"], path
        name = capture["artifacts"][0]["file"]
        with Image.open(root / name) as image:
            composed = image.convert("RGB")
        with Image.open(root / capture["artifacts"][1]["file"]) as image:
            guest = image.convert("RGB")
        assert guest.size == (800, 600)
        factor = capture["scale"] * capture["host_pixel_density"]
        assert composed.size == (round(800 * factor), round(600 * factor))

        # Positive device-edge ties round toward zero, independently of GPUI
        # component layout. Expand each original guest pixel's own edges.
        def snap(value):
            return math.ceil(value * factor - 0.5)

        ppc = capture["mode"] == "ppc"
        regions = (
            ("loadout-text", (295 if ppc else 305, 153, 419, 169)),
            ("theme-text", (285 if ppc else 295, 189, 370, 205)),
            ("loadout-arrow", (422, 153, 437, 170)),
            ("theme-arrow", (422, 189, 437, 206)),
        )
        for label, (left, top, right, bottom) in regions:
            grey = (capture["mode"] == "colour"
                    and capture["state"] == "disabled" and label.endswith("text"))
            ink = (150, 150, 150) if grey else (0, 0, 0)
            expected = {
                (xx, yy)
                for y in range(top, bottom) for x in range(left, right)
                if guest.getpixel((x, y)) == ink
                for yy in range(snap(y), snap(y + 1))
                for xx in range(snap(x), snap(x + 1))
            }
            actual = {
                (x, y)
                for y in range(snap(top), snap(bottom))
                for x in range(snap(left), snap(right))
                if composed.getpixel((x, y)) == ink
            }
            assert expected, (name, label, "empty guest mask")
            assert actual == expected, (
                name, label, "missing", len(expected - actual),
                "extra", len(actual - expected),
            )
            regions_checked += 1
        if capture["state"] == "disabled":
            scale_label = f'{round(capture["scale"] * 100):03d}'
            previous = root / (
                f'popup-disabled-canonical-{capture["mode"]}-{scale_label}.guest.png'
            )
            with Image.open(previous) as image:
                assert image.convert("RGB").tobytes() == guest.tobytes(), name
    print(f"Verified 72 artifacts, source fingerprints, {regions_checked} exact "
          "ink regions and 12 unchanged disabled guest rasters.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    verify(parser.parse_args().manifest)
