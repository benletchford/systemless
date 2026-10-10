#!/usr/bin/env python3
"""Check caret/selection visibility against native fixture change bounds.

Antialiased glyphs are not compared with a binary font oracle. This verifies
state changes and hidden-state consistency within the shared field only.
"""
import argparse
import importlib.util
import json
import math
from pathlib import Path

from PIL import Image, ImageChops


def verify(directory):
    checker = Path(__file__).with_name("verify-gpui-smooth-styled-provenance.py")
    spec = importlib.util.spec_from_file_location("provenance", checker)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    assert module.verify(directory) == 192
    manifest = json.loads((directory / "progress.json").read_text())
    assert manifest["matrix_kind"] == "single-line"
    cases = {(c["kind"], c["mode"], c["state"], c["insertion_offset"], c["scale"]): c
             for c in manifest["cases"]}
    top, left, bottom, right = manifest["field_bounds"]

    def field(case, kind):
        factor = 1 if kind == "guest" else case["scale"] * case["device_scale"]
        box = (math.floor(left * factor), math.floor(top * factor),
               math.ceil(right * factor), math.ceil(bottom * factor))
        with Image.open(directory / case[kind]) as image:
            return image.convert("RGB").crop(box), factor, box

    checks = []
    for mode in ["mono", "colour", "ppc8", "ppc16"]:
        for scale in [0.75, 1, 1.5, 2]:
            for kind, offset, visible, hidden in [
                ("selection", None, "selected", "suspended"),
                ("caret", 0, "visible", "blink-off"),
                ("caret", 26, "visible", "blink-off"),
            ]:
                before = cases[kind, mode, visible, offset, scale]
                after = cases[kind, mode, hidden, offset, scale]
                a, _, _ = field(before, "guest")
                b, _, _ = field(after, "guest")
                native = ImageChops.difference(a, b).getbbox()
                assert native is not None, "native visibility state did not change"
                a, factor, box = field(before, "rendered")
                b, _, _ = field(after, "rendered")
                changed = ImageChops.difference(a, b).getbbox()
                assert changed is not None, "composed visibility state did not change"
                # Allow one device pixel for fractional edge sampling.
                bounds = (math.floor((left + native[0]) * factor) - box[0] - 1,
                          math.floor((top + native[1]) * factor) - box[1] - 1,
                          math.ceil((left + native[2]) * factor) - box[0] + 1,
                          math.ceil((top + native[3]) * factor) - box[1] + 1)
                assert bounds[0] <= changed[0] < changed[2] <= bounds[2]
                assert bounds[1] <= changed[1] < changed[3] <= bounds[3]
                if kind == "caret":
                    suspended = cases[kind, mode, "suspended", offset, scale]
                    for image_kind in ["guest", "rendered"]:
                        hidden_image, _, _ = field(after, image_kind)
                        suspended_image, _, _ = field(suspended, image_kind)
                        assert hidden_image.tobytes() == suspended_image.tobytes(), (
                            mode, scale, offset, image_kind, "hidden/suspended field differs")
                checks.append({"kind": kind, "mode": mode, "scale": scale,
                               "offset": offset, "native_change_bounds": native,
                               "composed_change_bounds": changed})
    return checks


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--report", type=Path)
    args = parser.parse_args()
    checks = verify(args.directory)
    if args.report:
        args.report.write_text(json.dumps({"checks": checks, "scope": __doc__}, indent=2) + "\n")
    print(f"{len(checks)} visibility changes verified; hidden caret and suspended fields match")
