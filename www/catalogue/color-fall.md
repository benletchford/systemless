---
id: color-fall
kind: game
title: ColorFall
summary: Arrange falling four-color blocks to clear matching rows and columns.
developer: John V. Holder
publisher: John V. Holder
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened in 68K Systemless. A new game showed a
      falling four-color block; the configured 4 key moved it left.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3737
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and the unchanged archive once. ColorFall
      began active play, accepted the 4 key for left movement, and advanced 301 guest
      ticks across a five-second sample at about 60 ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3737
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 37847a4d020d1b705182319bf48e142ee04f299ca15d43e4de46de71f2d35752
    size_bytes: 154219
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/color-fall-111.hqx
    rights_holder: John V. Holder
    permission: >-
      The bundled Help Manual permits uploading ColorFall and distributing it on CDs
      so long as both documents accompany the game; it requests a copy of any CD. This
      entry uses the complete unchanged archive with the Help Manual, Read Me, and
      registration program.
    notes: >-
      Original 154,219-byte BinHex/StuffIt archive, SHA-256
      37847a4d020d1b705182319bf48e142ee04f299ca15d43e4de46de71f2d35752.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 77d3b9b9bfe5b4afd887bb1e97439de3f65610517b6de129bb2aa7fbbbaca55d
    size_bytes: 2858
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3737
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware game for this
      catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 388-by-313 game-content crop at (206,154) from an 800-by-600 Systemless
      framebuffer after moving the block left.
references:
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/color-fall-111.hqx
---

## Match colors as they fall

![A falling block in ColorFall](https://assets.systemless.org/catalogue/media/sha256/77/77d3b9b9bfe5b4afd887bb1e97439de3f65610517b6de129bb2aa7fbbbaca55d.png)

Start a new game from the File menu. Use **4** to move left, **6** to move
right, **5** to rotate, and **2** to drop. The keys can be changed from the
Options menu. Arrange matching colors to clear pieces and score points.
