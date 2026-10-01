---
id: enemy-bomber-balloons
kind: game
title: Enemy Bomber Balloons
summary: Defend your cannon against waves of enemy bomber balloons.
developer: Chess Piece Face
publisher: Chess Piece Face
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened in 68K Systemless. Pressing Space
      dismissed the controls page and started the live cannon playfield.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3725
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once. A click started the round, Right
      moved the cannon across the ground, and the active playfield showed advancing
      balloons. A five-second sample measured 60.0 host frames/s, 60.2 guest ticks/s, an 18.0
      ms maximum frame, and a 131 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3725
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: ccd48628c51416f17014dff233c22e7255eff4e6d09be42df043859d0ef23034
    size_bytes: 437579
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/enemy-bomber-balloons-10.hqx
    rights_holder: Chess Piece Face
    permission: >-
      The application resource fork permits use and distribution free of charge,
      forbids sale for profit, and requires the application and accompanying information to
      remain unaltered. This is the unchanged original archive.
    notes: >-
      Original 437,579-byte BinHex/StuffIt archive, SHA-256
      ccd48628c51416f17014dff233c22e7255eff4e6d09be42df043859d0ef23034.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7ad6008d8aeebd46162dcfc99727e1436f5dca2389e09099f606446d3335432c
    size_bytes: 30157
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3725
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freely distributable game
      for this catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 614-by-496 game-content crop at (333,354) from a 1280-by-900 Chrome
      screenshot during an active round after moving the cannon right.
references:
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/enemy-bomber-balloons-10.hqx
---

## Defend the ground

![Enemy Bomber Balloons cannon playfield](https://assets.systemless.org/catalogue/media/sha256/7a/7ad6008d8aeebd46162dcfc99727e1436f5dca2389e09099f606446d3335432c.png)

Move the cannon with the arrow keys and press Space to fire at the incoming
balloons. Press Escape to pause or resume. This original freeware version runs
on 68020 and later Macintosh computers.
