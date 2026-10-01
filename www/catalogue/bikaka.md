---
id: bikaka
kind: game
title: Bikaka 1.4
summary: Arrange falling colored pieces on a hexagonal grid and clear rows.
developer: Ingemar Ragnemalm
publisher: Ingemar Ragnemalm
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
runtime:
  runtime_pacing:
    cpu_mhz: 10
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive auto-started a live hexagonal falling-piece
      board in 68K Systemless. Its configured M and minus keys shifted a piece
      left and right. Space dropped it, raised the score from 0 to 6,
      incremented the piece statistics, and brought in a new piece.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3760
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive
      once. At 10 MHz, browser M and minus input shifted the falling piece;
      Space dropped it, raised the score from 1 to 5, and brought in the next
      piece. A five-second active sample measured about 60 host frames and
      60 guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3760
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/bikaka-14.hqx
    expected_sha256: d50b5d50407304d742a72789038678025e7b5fc537d1f383d44965739c7cbfce
    expected_size: 173201
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/bikaka-14.hqx
    rights_holder: Ingemar Ragnemalm
    permission: >-
      The bundled Bikaka 1.4 documentation calls the game freeware and permits
      copying and use without charge for non-profit purposes. It prohibits
      commercial distribution without written permission and asks that
      modified copies not be spread. This entry preserves the complete
      original archive and its documentation.
    notes: >-
      Original 173,201-byte BinHex/StuffIt archive, SHA-256
      d50b5d50407304d742a72789038678025e7b5fc537d1f383d44965739c7cbfce.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/bikaka/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3760
    permission: >-
      Fresh Systemless gameplay capture from the author's freeware release.
      Underlying artwork remains its owner's property.
    notes: >-
      Exact 495-by-330 game-content crop at (40,20) from an 800-by-600
      Systemless framebuffer after moving and dropping one piece; score 6.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/bikaka-14.hqx
---

## Arrange falling hexagons

![Bikaka board after dropping a piece](incoming/bikaka/gameplay.png)

The game starts as soon as it opens. Move the falling piece with **M** and
**-**, rotate it with **,** and **.**, and press **Space** to drop it. Choose
**Configure keys** from the **File** menu to change the controls.
