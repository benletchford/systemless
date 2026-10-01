---
id: classic-daleks
kind: game
title: Classic Daleks
summary: Evade pursuing Daleks and lure them into collisions on a turn-based board.
developer: Ingemar Ragnemalm
publisher: Ingemar Ragnemalm
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened in 68K Systemless. Cmd-N started a live
      board; two mouse-directed turns moved the player one cell right and advanced the
      pursuing Daleks.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3734
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once. Cmd-N started a live board; two
      mouse-directed turns moved the player right and advanced the Daleks. A five-second
      active-board sample measured 60.0 host frames/s, 60.2 guest ticks/s, a 4.2 ms
      maximum frame, and a 131 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3734
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 603cbf7606a7ff14ca4dd9721fbbea967333f23b96efed934cf4649ff41c4710
    size_bytes: 48208
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/classic-daleks-12.hqx
    rights_holder: Ingemar Ragnemalm
    permission: >-
      Section 5 of the bundled Classic Daleks documentation calls the game freeware
      and invites giving it to friends. It asks for a reference copy for distribution
      that is not strictly nonprofit. This entry uses the complete unchanged archive,
      including that documentation.
    notes: >-
      Original 48,208-byte BinHex/StuffIt archive, SHA-256
      603cbf7606a7ff14ca4dd9721fbbea967333f23b96efed934cf4649ff41c4710.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: e69b49aee01db88fe5ad0f92b309885560854dda6f63ede3e667531e1fbc49c3
    size_bytes: 8000
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3734
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware game for this
      catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 620-by-459 game-content crop at (330,321) from a 1280-by-900 Chrome
      screenshot after two mouse-directed turns.
references:
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/classic-daleks-12.hqx
---

## Outwit the Daleks

![A live Classic Daleks board after two turns](https://assets.systemless.org/catalogue/media/sha256/e6/e69b49aee01db88fe5ad0f92b309885560854dda6f63ede3e667531e1fbc49c3.png)

Choose **New game** from the File menu. Move with the keypad or click the
nearby direction you want to take. The Daleks follow each turn; make them
collide with one another while keeping clear of their path.
