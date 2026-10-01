---
id: bert
kind: game
title: Bert 1.1
summary: Hop across a pyramid of tiles while dodging falling balls and roaming enemies.
developer: Ingemar Ragnemalm
publisher: Ingemar Ragnemalm
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
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened the title screen and a live pyramid in
      68K Systemless. Game > New game started Level 1A. Six alternating numeric-keypad 1
      and 2 presses moved Bert down the pyramid and raised the score from 1 to 7 while
      an enemy ball crossed the board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3758
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once.
      Game > New game opened Level 1A; real browser numeric-keypad 1 and 2 input moved
      Bert across the pyramid and raised the score from 1 to 4. An enemy encounter
      reduced lives from 3 to 2 while play continued. A five-second active sample measured
      about 60 host frames and 60 guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3758
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: f80c911e06d35be898073db9f36d09e94df3e2793188e9397ae925b4b3d47e1e
    size_bytes: 313150
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.lysator.liu.se/~ingemar/games/news.html
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/bert-11.hqx
    rights_holder: Ingemar Ragnemalm
    permission: >-
      The author's games news explicitly declares Bert freeware with no restrictions
      on distribution. This is the complete original 1.1 BinHex/StuffIt package,
      including the author's documentation.
    notes: >-
      Original 313,150-byte archive, SHA-256
      f80c911e06d35be898073db9f36d09e94df3e2793188e9397ae925b4b3d47e1e.
      The promoted public object was fetched back and matched this hash and size.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8968d83a4706b8c1179d1c9e4a0591c0185eeb6293908588d53eb9347dfe5c96
    size_bytes: 7113
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3758
    permission: >-
      Fresh Systemless gameplay capture from the author's freeware game. Underlying
      artwork remains its owner's property.
    notes: >-
      Exact 490-by-330 game-content crop at (155,125) from an 800-by-600 Systemless
      framebuffer after four numeric-keypad hops; score 5. The promoted public
      media object was fetched back and matched its submitted hash and size.
references:
- https://www.lysator.liu.se/~ingemar/games/news.html
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/bert-11.hqx
---

## Hop across the pyramid

![Bert navigating the pyramid](https://assets.systemless.org/catalogue/media/sha256/89/8968d83a4706b8c1179d1c9e4a0591c0185eeb6293908588d53eb9347dfe5c96.png)

Choose **New game** from the **Game** menu. Use the numeric keypad's
**1**, **2**, **4**, and **5** keys to hop between tiles. Change every tile
while avoiding enemies; a spinning disk can carry Bert back to the top.
