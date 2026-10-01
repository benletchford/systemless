---
id: space-invader-104
kind: game
title: SpaceInvader! 1.04
summary: Dodge enemy ships and bombs while shooting through an escalating space battle.
developer: Hui Dong
publisher: Hui Dong
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
    systemless_version: 0.72.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged unregistered shareware package reached Level 1 in 68K Systemless.
      At the same guest tick, holding Right moved the player ship from center while
      the idle run left it there. Holding Space fired a bullet and spent one score point,
      as the in-game instructions describe. The release browser fetched the archive
      once, reached Level 1, showed the ship moving right on ArrowRight, and displayed a
      player bullet with the score changing from zero to minus one after Space.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3820
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: c84f7eb3d5a5a5e5e91543691f7fdaa69f5470ae01e49b8095c4cc9e2c725294
    size_bytes: 338548
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/space-invader-104.hqx
    rights_holder: Hui Dong
    permission: >-
      The bundled author ReadMe asks people to distribute only the unregistered
      shareware copy. This unchanged original archive displays the unregistered notice and
      includes the ReadMe.
    notes: >-
      Original 338,548-byte BinHex/StuffIt archive, SHA-256
      c84f7eb3d5a5a5e5e91543691f7fdaa69f5470ae01e49b8095c4cc9e2c725294.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 69c53224d8fcb6030e9db18be42cf576ccdd677621763311b385324a8b692152
    size_bytes: 3726
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3820
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only crop of the release browser's active Level 1 playfield; emulator
      framing is excluded.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/space-invader-104.hqx
---

## Defend the playfield

![SpaceInvader! Level 1 battle](https://assets.systemless.org/catalogue/media/sha256/69/69c53224d8fcb6030e9db18be42cf576ccdd677621763311b385324a8b692152.png)

Click **OK** on the unregistered notice, then click **Play**. Move with the
left and right arrow keys or **[** and **]**; press **Space** to fire. Shots
cost a point, so pick your targets as enemy ships and bombs descend.
