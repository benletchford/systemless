#!/usr/bin/env python3
"""Compare styled TextEdit GPUI ink to device-snapped guest pixels.

This verifies only the captured field, not surrounding chrome, caret,
interactions or arbitrary background/transfer policies.
"""
import argparse
import hashlib
import json
import math
import subprocess
from pathlib import Path
from PIL import Image


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify_case(root, case, bounds):
    rendered_path = root / case["rendered"]
    guest_path = root / case["guest"]
    for path, key in [(rendered_path, "rendered_sha256"), (guest_path, "guest_sha256")]:
        if key in case and sha256(path) != case[key]:
            raise AssertionError(f"capture hash differs: {path}")
    with Image.open(rendered_path) as image:
        rendered = image.convert("RGB")
    with Image.open(guest_path) as image:
        guest = image.convert("RGB")
    factor = case["scale"] * case["device_scale"]
    expected_size = tuple(round(value * factor) for value in guest.size)
    assert rendered.size == expected_size, (case["rendered"], rendered.size, expected_size)
    # GPUI bitmap edge snapping uses nearest device pixel with half ties
    # toward zero. Coordinates here are nonnegative scene coordinates.
    snap = lambda value: math.ceil(value * factor - 0.5)
    top, left, bottom, right = bounds
    crop = (snap(left), snap(top), snap(right), snap(bottom))
    expected = Image.new("RGB", (crop[2] - crop[0], crop[3] - crop[1]), "white")
    source = guest.load()
    target = expected.load()
    for y in range(top, bottom):
        for x in range(left, right):
            color = source[x, y]
            for dy in range(snap(y) - crop[1], snap(y + 1) - crop[1]):
                for dx in range(snap(x) - crop[0], snap(x + 1) - crop[0]):
                    target[dx, dy] = color
    actual = rendered.crop(crop)
    mismatch = sum(a != b for a, b in zip(actual.getdata(), expected.getdata()))
    assert mismatch == 0, f"{case['rendered']}: {mismatch} field pixels differ"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("manifest", type=Path)
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text())
    repo = Path(__file__).resolve().parents[2]
    if "fixture" in manifest:
        assert sha256(repo / manifest["fixture"]) == manifest["fixture_sha256"], "fixture hash differs"
    if "source_sha256" in manifest:
        for path, expected in manifest["source_sha256"].items():
            source = subprocess.check_output(["git", "show", f"{manifest['source_commit']}:{path}"], cwd=repo)
            assert hashlib.sha256(source).hexdigest() == expected, f"pinned source hash differs: {path}"
    cases = manifest["cases"]
    states = manifest.get("states", ["inactive"])
    assert states and len(states) == len(set(states)), "missing or duplicate states"
    assert set(states) <= {"inactive", "selected", "suspended", "resumed"}, "unknown qualification state"
    required = {(mode, scale, state) for mode in ["mono", "colour", "ppc16", "ppc8"]
                for scale in [0.75, 1.0, 1.5, 2.0] for state in states}
    actual = {(case["mode"], case["scale"], case.get("state", "inactive")) for case in cases}
    assert len(cases) == len(required) and actual == required, "incomplete or duplicate CPU/scale matrix"
    assert all(case["device_scale"] == 2 for case in cases), "unqualified device density"
    for case in cases:
        verify_case(args.manifest.parent, case, manifest["field_bounds"])
    print(f"verified {len(manifest['cases'])} styled field captures")


if __name__ == "__main__":
    main()
