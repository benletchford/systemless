---
id: floor-tiles-201
kind: game
title: FloorTiles 2.0.1
summary: Place four-color tiles on a grid and match edges for points.
developer: Karl Bunker
publisher: Karl Bunker
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.73.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac package opened its 68K application. After dismissing
      the startup and shareware reminders, Start Tiles began a Match Sides board.
      Clicking a grid cell placed a colored tile and raised the score from 0 to 5.
      The release browser fetched the same archive once; its board click placed a
      tile and raised the score from 0 to 1.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3849
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/floor-tiles-201.hqx
    expected_sha256: ff03c52b5e0e6c03bac7b24a9273fc3ecedd55715fe8c90d345ff6355fc4295f
    expected_size: 238197
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/floor-tiles-201.hqx
    rights_holder: Karl Bunker
    permission: >-
      Karl Bunker's bundled ReadMe permits free distribution of FloorTiles when
      all included files remain unchanged and together, without unreasonable
      charges. It explicitly welcomes CD-ROM and private sharing.
    notes: >-
      Original 238,197-byte Info-Mac BinHex/StuffIt package with application,
      ReadMe, and registration utility; SHA-256
      ff03c52b5e0e6c03bac7b24a9273fc3ecedd55715fe8c90d345ff6355fc4295f.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/floor-tiles-201/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3849
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareable package.
      Underlying artwork remains its owner's property.
    notes: >-
      Exact 504-by-314 game-content crop at (148,162) from an 800-by-600 Systemless
      framebuffer after placing the first tile; emulator framing excluded.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/floor-tiles-201.hqx
---

## Match the sides

![FloorTiles Match Sides board after a scored placement](incoming/floor-tiles-201/gameplay.png)

Dismiss the startup prompts with **OK** and **Not Yet**, then click
**Start Tiles**. Click a square to place the colored tile. Match its edges
with neighboring tiles to build your score. The game includes three related
rulesets and offers board size and timer choices from its menus.
