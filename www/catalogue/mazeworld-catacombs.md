---
id: mazeworld-catacombs
kind: game
title: Mazeworld Catacombs 1.0.5
summary: Explore a first-person maze, collect jewels, and evade monsters.
developer: Farfetch Software
publisher: Farfetch Software
year: 1997
architectures:
- 68k
default_architecture: 68k
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged final freeware release opened its title window and then entered
      an active first-person maze in 68K Systemless. At the same guest tick, holding the
      documented keypad 9 turn key rotated the corridor viewpoint while the idle run
      left it fixed. The release browser build fetched the archive once, entered the
      maze, and turned the corridor view with the same keypad key while life and time
      remained full.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3812
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: d37a0996b5b6fcf4dcbba0d0c56ea83d5b0f1527e88b811f9d67d0f984ddd25d
    size_bytes: 493753
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/mazeworld-catacombs-105.hqx
    rights_holder: Farfetch Software
    permission: >-
      The bundled final-release Software License expressly permits free distribution
      of the original, unmodified game with its documentation. This archive retains the
      game, Read Me, and companion sound file.
    notes: >-
      Original 493,753-byte BinHex/StuffIt archive, SHA-256
      d37a0996b5b6fcf4dcbba0d0c56ea83d5b0f1527e88b811f9d67d0f984ddd25d.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7f883ac88f91c217b68efce7731b44f13df87aaa40098bff8628db8b0254df6f
    size_bytes: 5052
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3812
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only release browser crop of the first-person maze and status display
      after a turn; Classic Mac and emulator framing are excluded.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/mazeworld-catacombs-105.hqx
---

## Find your way down

![Mazeworld Catacombs first-person maze](https://assets.systemless.org/catalogue/media/sha256/7f/7f883ac88f91c217b68efce7731b44f13df87aaa40098bff8628db8b0254df6f.png)

Press **Space** to enter the maze. Use the numeric keypad: **8** moves
forward, **2** moves back, **4** and **6** move sideways, and **7** and **9**
turn. Press **Shift** or **Space** to fire, and **Escape** to pause. Collect
jewels and shoot the pair of lava lamps to activate each level's exit.
