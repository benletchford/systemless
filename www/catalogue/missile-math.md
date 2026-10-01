---
id: missile-math
kind: game
title: Missile Math 1.0
summary: Defend launch control by targeting the place values on flying number snakes.
developer: Thornley Jobe
publisher: Thornley Jobe
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
    systemless_version: 0.72.0 + deterministic runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Missile Math 1.0 archive opened its New Player dialog and active
      Level 1 board in 68K Systemless. Pressing the documented 1 key launched a visible
      missile from launcher 1 in a matched deterministic replay; the idle replay
      showed no missile. The release browser build fetched the same archive once, reached
      active Level 1, and launched a missile with the same key.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3804
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: e0751ff12b7a6cd79326a8bae7481d8a9de2967c27b3441e996e5c56f3f55089
    size_bytes: 304319
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/missile-math-10.hqx
    rights_holder: Thornley Jobe
    permission: >-
      The bundled author-supplied Read Me calls Missile Math freeware and permits
      sharing the complete package for non-commercial purposes when all documentation is
      included. This entry preserves that package for free online access.
    notes: >-
      Original 304,319-byte BinHex package, SHA-256
      e0751ff12b7a6cd79326a8bae7481d8a9de2967c27b3441e996e5c56f3f55089.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d5e7dbafcf9e79e422dab32af552c85f2a16d26ea96d8db9a169f127fb8718d1
    size_bytes: 39860
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3804
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: Cropped Level 1 playfield, launchers, and status panel during a missile launch.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/missile-math-10.hqx
---

## Target the place value

![Missile Math Level 1](https://assets.systemless.org/catalogue/media/sha256/d5/d5e7dbafcf9e79e422dab32af552c85f2a16d26ea96d8db9a169f127fb8718d1.png)

Enter a player name and select **Play**. Each numbered launcher fires with
its matching number key. You can also move the mouse to choose a launcher
and click to fire. Aim at the vulnerable place value on each number snake.
The complete original package includes the Read Me and teacher information.
