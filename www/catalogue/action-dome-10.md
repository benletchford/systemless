---
id: action-dome-10
kind: game
title: Alien Action Dome 1.0
summary: >-
  Collect bowls of sugar worms while dodging strange enemies in this colorful
  arcade maze.
developer: Slovis Software
publisher: Slovis Software
year: 1995
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
      Dismissing the registration reminder and selecting Play reached Level 1,
      with the timer, collectibles, and moving enemies active. Matched runs showed
      Keypad 6 move Glerp right while idle left him at the start. The release
      browser reproduced the title menu, Level 1, and rightward movement.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3865
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/action-dome-10.hqx
    expected_sha256: 747c07cc039a71773b1218e8a03f1c39ca2130515605d2c35c312882c797c32f
    expected_size: 1707279
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/action-dome-10.hqx
    rights_holder: Slovis Software
    permission: >-
      The bundled Read Me permits distribution of the unregistered game when
      the application, Sprund file, Read Me, and Registration are kept together,
      and prohibits distribution for profit without Slovis Software's consent.
      This unchanged original archive contains all four files.
    notes: >-
      Original 1,707,279-byte Info-Mac BinHex/StuffIt package; SHA-256
      747c07cc039a71773b1218e8a03f1c39ca2130515605d2c35c312882c797c32f.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/action-dome-10/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3865
    permission: >-
      Fresh Systemless gameplay capture from the unchanged unregistered package.
      Underlying artwork remains its owner's property.
    notes: >-
      519-by-470 game-content crop at (48,20) from an 800-by-600 Systemless
      framebuffer on Level 1 after Keypad 6 moved Glerp right.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/action-dome-10.hqx
---

## Collect the sugar worms

![Alien Action Dome Level 1](incoming/action-dome-10/gameplay.png)

Dismiss the registration reminder with **Not Yet**, then select **Play** to
enter Level 1. Collect the bowls of sugar worms while avoiding the creatures.
The game's **Instructions** menu explains its characters and pickups.

Use **Keypad 8**, **Keypad 2**, **Keypad 4**, and **Keypad 6** to move; **Keypad 5** stops Glerp.
