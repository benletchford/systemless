#!/bin/sh
# Paired guest/composed captures from the same deterministic guest checkpoints.
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
output_dir=${1:-output/gpui-popups}
mkdir -p "$output_dir"
output_dir=$(CDPATH= cd -- "$output_dir" && pwd)
rm -f "$output_dir/manifest.json"
cd "$project_dir"

for mode in mono colour ppc; do
    case "$mode" in
        mono) set -- --screen-depth 1 ;;
        colour) set -- --screen-depth 8 ;;
        ppc) set -- --prefer-powerpc ;;
    esac
    for checkpoint in scrolled selected; do
        stem="$output_dir/$mode-$checkpoint"
        rm -f "$stem.png" "$stem.guest.png"
        cargo run --quiet --locked --no-default-features --features gpui-demo-test \
            --example gpui-menu-demo -- tests/toolbox-showcase/toolbox-showcase.sit \
            "$@" "--capture-popup-controls-$checkpoint" "$stem.png" \
            > "$stem.log" 2>&1
        test -s "$stem.png"
        test -s "$stem.guest.png"
    done
done

python3 - "$output_dir" tests/toolbox-showcase/toolbox-showcase.sit <<'PY'
import hashlib
import json
from pathlib import Path
import struct
import sys

output = Path(sys.argv[1])
captures = []
for mode in ("mono", "colour", "ppc"):
    for checkpoint in ("scrolled", "selected"):
        files = {}
        for kind, suffix in (("composed", ".png"), ("guest", ".guest.png")):
            path = output / f"{mode}-{checkpoint}{suffix}"
            data = path.read_bytes()
            if data[:8] != b"\x89PNG\r\n\x1a\n" or data[12:16] != b"IHDR":
                raise ValueError(f"Not a PNG capture: {path.name}")
            width, height = struct.unpack(">II", data[16:24])
            if not width or not height:
                raise ValueError(f"Empty capture: {path.name}")
            files[kind] = dict(file=path.name, width=width, height=height,
                               sha256=hashlib.sha256(data).hexdigest())
        captures.append(dict(mode=mode, checkpoint=checkpoint, files=files))
manifest = dict(schema_version=1,
                fixture_sha256=hashlib.sha256(Path(sys.argv[2]).read_bytes()).hexdigest(),
                captures=captures)
(output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
PY
