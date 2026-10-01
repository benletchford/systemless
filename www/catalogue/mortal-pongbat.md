---
id: mortal-pongbat
kind: game
title: Mortal Pongbat
summary: Defend your goal with a paddle amid ricocheting pucks, mines, and power-ups.
developer: David Hirschfield
publisher: David Hirschfield
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
      The original 68K application in the unchanged Info-Mac archive advanced through
      its intro to an active match. Holding the documented Shift key moved the blue
      paddle upward.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3742
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once. A
      live match began; holding Shift moved the blue paddle upward. A five-second
      active-play sample ran at about 60 guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3742
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 18fd463ee79834ac4d5cc90d8b1a924b2be7c55a9d1ae1b0bfc14aaa4c820e4c
    size_bytes: 693184
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/mortal-pongbat-141.hqx
    rights_holder: David Hirschfield
    permission: >-
      The author's bundled About MPB document identifies Mortal Pongbat as shareware
      and asks players who enjoy it to send a letter and a voluntary payment. This is
      the complete, unchanged public Info-Mac distribution, including that document and
      the original 68K application.
    notes: >-
      Original 693,184-byte BinHex/StuffIt archive, SHA-256
      18fd463ee79834ac4d5cc90d8b1a924b2be7c55a9d1ae1b0bfc14aaa4c820e4c.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 27778ea6a0fa6c24627726b3fa5e0cf168386f47f9f8e9028894bc137a8cf399
    size_bytes: 50830
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3742
    permission: >-
      Fresh Systemless gameplay capture from the author's original shareware release
      for this catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 640-by-480 game-content crop at (80,60) from an 800-by-600 Systemless
      framebuffer after moving the blue paddle upward.
references:
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/mortal-pongbat-141.hqx
---

## Defend your side of the arena

![An active Mortal Pongbat match](https://assets.systemless.org/catalogue/media/sha256/27/27778ea6a0fa6c24627726b3fa5e0cf168386f47f9f8e9028894bc137a8cf399.png)

Choose **Play** from the title screen. The blue paddle uses **Shift** to move
up, **Control** to move down, and **Z** to fire. The red paddle uses keypad
**9** and **3** to move and keypad **4** to fire. Open **Keys** to change the
controls or enable mouse movement.
