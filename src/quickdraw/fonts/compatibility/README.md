# Classic layout compatibility data

Systemless uses bundled URW outlines for fallback font pixels. This directory
contains separately licensed logical metrics used where a classic-compatible
layout is required. It does not contain Apple font software or glyph artwork.

## Geneva 9 advances

`geneva9-advances.bin` contains 95 one-byte logical advances in Mac Roman ASCII
order, from `0x20` through `0x7E`. Its SHA-256 is
`44262c59655597d527318a7ba9b62350defba4f556cb97172d0fea05a6f0fd35`.

The values come exclusively from the `advance` fields in Systemless's
historical OFL-licensed Kurrajong 9 source:

- public commit: `0540ef827d800fbdb00eafc67922cef3245b81e5`
- path: `src/quickdraw/fonts/pixel_font/geneva9.rs`
- Git blob: `afc3cc9ec393dd1c6341c6375aedb95d3d106cd0`
- source SHA-256:
  `2578381b3fcf9f3bde359d1b258bcf0b95fcff8ea3181863d0c395c7563bfada`
- copyright: Copyright (c) 2026 Ben Letchford
- licence: SIL Open Font License 1.1
- Reserved Font Name: Systemless

PR #1524 later removed the old bitmap catalogue. The compatibility component
retains only the 95 advance values from that licensed Systemless source. It
excludes the historical bitmap pixels, masks, bearings, origins, heights, and
all extended characters. Current URW font files remain the source of raster
pixels, bearings, coverage, and extended Mac Roman glyphs.

To verify the public source and component:

```sh
git show 0540ef827d800fbdb00eafc67922cef3245b81e5:src/quickdraw/fonts/pixel_font/geneva9.rs > /tmp/geneva9.rs
python3 - /tmp/geneva9.rs /tmp/geneva9-advances.bin <<'PY'
import pathlib
import re
import sys

source = pathlib.Path(sys.argv[1]).read_text()
advances = [int(value) for value in re.findall(r"g!\(\s*(\d+)", source)]
assert len(advances) == 95
pathlib.Path(sys.argv[2]).write_bytes(bytes(advances))
PY
shasum -a 256 /tmp/geneva9.rs /tmp/geneva9-advances.bin
cmp /tmp/geneva9-advances.bin src/quickdraw/fonts/compatibility/geneva9-advances.bin
```

The source file should hash to `2578381b...` and the component to
`44262c59...`. Extracting the first numeric argument from each of its 95 `g!`
records, in source order, reproduces `geneva9-advances.bin` exactly.

See [OFL.txt](OFL.txt) for the component's copyright and licence notice.

## Monaco maximum advance

The bundled Monaco substitute retains its own outline pixels and per-glyph
advances. Its `widMax` compatibility metric is derived at each requested size
from the classic scalable Monaco family's normalized maximum advance of
1552/2048 em. This is metadata only; no original font software or glyph artwork
is included.
