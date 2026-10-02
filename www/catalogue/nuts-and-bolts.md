---
id: nuts-and-bolts
kind: game
title: Nuts and Bolts
summary: >-
  Guide a roller-skating robot through colorful platforms to collect nuts and
  bolts while avoiding roaming creatures.
developer: Ronan Dowling
publisher: BigFishLittleFish Software
year: 1998
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.73.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged unregistered Info-Mac package opened in 68K Systemless. After
      dismissing the shareware reminder and selecting Play, Level 1 displayed its score,
      bonus timer, lives, moving creature, and robot. Matched runs showed Keypad 6 move
      the robot right while idle left it at the starting position. The release browser
      fetched the archive once and reached Level 1. At the same elapsed time, keypad
      input moved the robot across the lower platform while an untouched browser run left
      it at the starting position.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3874
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 745bced442defab437971d8dee6ae4a1bf865c66a3d48c2ee2646b82dddc2ae7
    size_bytes: 663293
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/nuts-and-bolts.hqx
    rights_holder: Ronan Dowling
    permission: >-
      Ronan Dowling's author-submitted Info-Mac abstract explicitly grants inclusion
      of this game on Info-Mac CDs. The bundled Read Me identifies this unchanged
      package as the unregistered shareware version with a 30-day trial and the first ten
      levels available.
    notes: >-
      Original 663,293-byte Info-Mac BinHex/StuffIt package; SHA-256
      745bced442defab437971d8dee6ae4a1bf865c66a3d48c2ee2646b82dddc2ae7.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 1aeeb889029387960505c732f2a6bb137b98c18c14a9f60d059902f0c16e1866
    size_bytes: 60773
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3874
    permission: >-
      Fresh Systemless gameplay capture from the unchanged unregistered package.
      Underlying artwork remains its owner's property.
    notes: >-
      Exact 640-by-480 content crop at (80,60) from an 800-by-600 Systemless
      framebuffer on Level 1 after Keypad 6 moved the robot right.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/nuts-and-bolts.hqx
---

## Skate through the platforms

![Nuts and Bolts Level 1 after moving right](https://assets.systemless.org/catalogue/media/sha256/1a/1aeeb889029387960505c732f2a6bb137b98c18c14a9f60d059902f0c16e1866.png)

Dismiss the shareware reminder with **Not Yet**, then choose **Play**.
Use **Keypad 4** and **Keypad 6** to move along the platforms and collect
nuts and bolts. The unregistered copy includes a 30-day trial of its first
ten levels.
