---
id: blobbo-lite
kind: game
title: Blobbo Lite
summary: Solve 25 luck-free strategy puzzles in Glenn Andreas's complete promotional game.
developer: Glenn Andreas Software
publisher: Glenn Andreas Software
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
      The unchanged Info-Mac Blobbo 1.0.2 Lite archive opened Level 1 in 68K
      Systemless. Dismissed its opening dialog and pressed Right; Blobbo moved
      one tile right on the board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3721
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once and opened Level 1. Pressing
      Right moved Blobbo one tile right. A five-second active-board sample
      measured 60.0 host frames/s, 59.8 guest ticks/s, a 12.8 ms maximum
      frame, and a 132 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3721
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/adv/blobbo-102-lite.hqx
    expected_sha256: 3d8dfcd0b33edca34e888c1f687b04a4f2d4442b093a14aa3894320bab990c21
    expected_size: 398044
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/adv/blobbo-102-lite.hqx
    rights_holder: Glenn Andreas
    permission: >-
      The bundled Read Me says Blobbo Lite may be freely distributed. It calls
      this a complete 25-level promotional game for the registered version,
      which adds a level editor and solution tools. This entry uses the
      unchanged archive, including its Read Me and three saved games.
    notes: >-
      Original 398,044-byte BinHex/StuffIt archive, SHA-256
      3d8dfcd0b33edca34e888c1f687b04a4f2d4442b093a14aa3894320bab990c21.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/blobbo-lite/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3721
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freely distributable
      game for this catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 514-by-343 game-window crop at (143,132) from an 800-by-600
      Systemless framebuffer after pressing Right in Level 1.
references:
- https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/adv/blobbo-102-lite.hqx
---

## Twenty-five levels to solve

![Blobbo at the start of Level 1 after moving right](incoming/blobbo-lite/gameplay.png)

Guide the yellow Blobbo through each room with the arrow keys. The goal is to
collect the toys while avoiding traps. This Lite version includes all 25 levels
and supports saving and restoring a game. The registered version adds a level
editor and solution recording.
