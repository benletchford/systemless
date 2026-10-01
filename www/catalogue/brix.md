---
id: brix
kind: game
title: Brix 1.0.2
summary: Steer a paddle to break rows of bricks before the clock runs out.
developer: Hugh Wilson
publisher: Hugh Wilson
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
      The unchanged Info-Mac shareware archive launched in 68K Systemless. File > New
      Game opened a live board. Moving the mouse from left to right shifted the paddle
      across the screen; the ball moved, bricks disappeared, and the score increased
      from 50 to 150.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3764
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once.
      File > New Game opened the board; real browser mouse moves shifted the paddle, the
      ball struck bricks, and the score increased from 0 to 150. A five-second active
      sample measured about 60 host frames and 60 guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3764
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: dac77eb9adb1edb49b85700d3d917aba4711c798875716dc82823b6957c1d041
    size_bytes: 595191
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/brix-102.hqx
    rights_holder: Hugh Wilson
    permission: >-
      Hugh Wilson submitted this original shareware package to Info-Mac for public
      distribution. Its bundled Read Me identifies the distributed edition as limited to
      four levels; registration unlocks sixteen more. This entry preserves the
      unchanged public demo package and Read Me.
    notes: >-
      Original 595,191-byte BinHex/StuffIt archive, SHA-256
      dac77eb9adb1edb49b85700d3d917aba4711c798875716dc82823b6957c1d041.
      The promoted public object was fetched and confirmed byte-for-byte identical
      to the original download on 2026-10-02.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 479dedd696af1b58949c123c013262712c51f7fa8e027affd3cb4774fb89851c
    size_bytes: 2157
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3764
    permission: >-
      Fresh Systemless gameplay capture from the original shareware demo. Underlying
      artwork remains its owner's property.
    notes: >-
      Exact 530-by-435 game-content crop at (135,95) from an 800-by-600 Systemless
      framebuffer after multiple bricks were cleared; score 150. The promoted
      public PNG was fetched and matched its 2,157-byte SHA-256 source on 2026-10-02.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/brix-102.hqx
---

## Break the bricks

![Brix board after clearing several bricks](https://assets.systemless.org/catalogue/media/sha256/47/479dedd696af1b58949c123c013262712c51f7fa8e027affd3cb4774fb89851c.png)

Choose **New Game** from the **File** menu to begin. Move the mouse to steer
its paddle, keeping the ball in play as the clock counts down. This original
shareware edition includes four levels.
