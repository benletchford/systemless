---
id: megapong
kind: game
title: MegaPong World Tournament
summary: Rally a golden ball with neon paddles in a colorful Pong match.
developer: JFX Studios
publisher: JFX Studios
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
      The unchanged JFX Studios freeware package launched in 68K Systemless.
      File > New Game started a match with a moving ball and both paddles.
      Matched deterministic runs showed Q move the left paddle up and A move
      it down while the idle run left it centered. The optional Set Keys dialog
      stopped the CPU at an illegal instruction; the main match used its default
      controls. The release browser build fetched the same archive once,
      started a match, and showed A move the left paddle down and Q move it
      back up while the ball remained in play.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3815
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/mega-pong.hqx
    expected_sha256: 123a773e69782a77fd9798096fd3d6fd398cab510af0df30085a4493b23abb51
    expected_size: 790530
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/mega-pong.hqx
    rights_holder: JFX Studios
    permission: >-
      The bundled JFX Studios ReadMe permits free non-profit distribution.
      It requires prior written consent for for-profit CD-ROM or similar
      distribution. This entry retains the complete original package for
      free online play.
    notes: >-
      Original 790,530-byte BinHex/StuffIt archive, SHA-256
      123a773e69782a77fd9798096fd3d6fd398cab510af0df30085a4493b23abb51.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/megapong/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3815
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only release browser crop with the ball and player paddle after
      moving down; Classic Mac menu bar and emulator framing are excluded.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/mega-pong.hqx
---

## Rally for the world

![MegaPong match and paddles](incoming/megapong/gameplay.png)

Choose **File → New Game** to start. In the default one-player setup, **Q**
moves the left paddle up and **A** moves it down. Keep the golden ball in play.
The **Set Keys** dialog currently fails in Systemless, so use the default
controls.
