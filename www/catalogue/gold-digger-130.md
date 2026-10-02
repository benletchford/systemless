---
id: gold-digger-130
kind: game
title: 'Gold Digger: The Lost Mines 1.3.0'
summary: Explore mine screens, climb ladders, and dodge hazards in a fast arcade adventure.
developer: T&T Software
publisher: T&T Software
year: 1996
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
      The unchanged unregistered Info-Mac package opened in 68K Systemless.
      After dismissing the shareware prompt, Gold Digger > New Game reached
      Screen 1 of The Official Lost Mines. At the same guest tick, Keypad 6
      moved the player right while an idle run left the player at the start.
      The release browser fetched the same archive once and reproduced
      rightward movement.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3857
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gold-digger-130.hqx
    expected_sha256: 8f9e5101f8864ae6938bf9c0efe9a582ed290134b0bbac7d067919145eda4cca
    expected_size: 943900
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gold-digger-130.hqx
    rights_holder: T&T Software
    permission: >-
      The bundled Read Me permits unregistered copies in shareware/demoware
      libraries when The Official Lost Mines screens and the Read Me accompany
      them. This unchanged archive contains the unregistered game, those screens,
      the Read Me, Novice Mines, and update notes.
    notes: >-
      Original 943,900-byte Info-Mac BinHex/StuffIt package; SHA-256
      8f9e5101f8864ae6938bf9c0efe9a582ed290134b0bbac7d067919145eda4cca.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/gold-digger-130/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3857
    permission: >-
      Fresh Systemless gameplay capture from the unchanged unregistered package.
      Underlying artwork remains its owner's property.
    notes: >-
      Exact 600-by-381 game-content crop at (100,120) from an 800-by-600
      Systemless framebuffer on Screen 1 after Keypad 6 moved the player right.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gold-digger-130.hqx
---

## Enter the lost mines

![Gold Digger Screen 1 after moving right](incoming/gold-digger-130/gameplay.png)

Dismiss the shareware reminder with **Not Yet**, then choose **New Game**
from the Gold Digger menu. Use **Keypad 4** and **Keypad 6** to move left
and right, **Keypad 8** and **Keypad 2** to climb, and **Keypad 5** to jump.
The bundled game documentation lists the other actions and hazards.
