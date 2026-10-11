#!/usr/bin/env python3
"""Reject pixel bugs that restoration equality alone cannot detect."""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile

from PIL import Image


def load(name):
    path = Path(__file__).with_name(name)
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("directory", type=Path)
    parser.add_argument("--report", type=Path, required=True)
    args = parser.parse_args()
    visibility = load("verify-gpui-styled-state-visibility.py")
    restoration = load("verify-gpui-styled-activation-pixels.py")
    assert len(visibility.verify(args.directory)) == 48
    manifest = json.loads((args.directory / "progress.json").read_text())
    rejected = []
    for bug in ["missing-composed-caret", "caret-leaks-on-suspend",
                "change-outside-caret", "missing-native-caret", "selection-leaks-on-suspend"]:
        with tempfile.TemporaryDirectory(prefix="systemless-visibility-negative-") as temporary:
            directory = Path(temporary) / "matrix"
            shutil.copytree(args.directory, directory)
            changed = copy.deepcopy(manifest)

            def case(kind, state):
                return next(c for c in changed["cases"] if c["kind"] == kind
                            and c["mode"] == "ppc16" and c["state"] == state
                            and c["scale"] == 1
                            and c["insertion_offset"] == (0 if kind == "caret" else None))

            def replace(target, source, image_kind):
                path = directory / target[image_kind]
                shutil.copy2(directory / source[image_kind], path)
                target[image_kind + "_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()

            visible = case("caret", "visible")
            if bug in ["missing-composed-caret", "missing-native-caret"]:
                image_kind = "guest" if bug == "missing-native-caret" else "rendered"
                for state in ["visible", "resumed"]:
                    replace(case("caret", state), case("caret", "blink-off"), image_kind)
            elif bug == "caret-leaks-on-suspend":
                replace(case("caret", "suspended"), visible, "rendered")
            elif bug == "selection-leaks-on-suspend":
                replace(case("selection", "suspended"), case("selection", "selected"), "rendered")
            else:
                path = directory / visible["rendered"]
                with Image.open(path) as image:
                    pixels = image.convert("RGBA")
                    top, left, bottom, right = changed["field_bounds"]
                    x, y = (right - 2) * 2, (bottom - 2) * 2
                    r, g, b, a = pixels.getpixel((x, y))
                    pixels.putpixel((x, y), (r ^ 1, g, b, a))
                    pixels.save(path)
                visible["rendered_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
                replace(case("caret", "resumed"), visible, "rendered")
            (directory / "progress.json").write_text(json.dumps(changed))
            assert len(restoration.verify(directory)) == 48, "negative must preserve restoration equality"
            try:
                visibility.verify(directory)
            except AssertionError:
                rejected.append(bug)
                print("Rejected " + bug, flush=True)
            else:
                raise AssertionError("visibility verifier accepted " + bug)
    args.report.write_text(json.dumps({"rejected": rejected,
        "scope": "Five deliberate pixel bugs with updated hashes preserve all48 restoration pairs but fail visibility checks. No font oracle is supplied."}, indent=2) + "\n")
