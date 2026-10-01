---
id: gunslinger
kind: game
title: Gunslinger 1.0
summary: Pick out armed targets in a photographed shootout scene.
developer: David McWherter Enterprises
publisher: David McWherter Enterprises
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
      The unchanged author-submitted package launched in 68K Systemless with
      its companion GunslingerRES file. File > New Game opened an active
      shootout. At the same guest tick, a visible target stayed on screen in
      the idle run but disappeared after a click on its position. The release
      browser build fetched the archive once, opened New Game, displayed a
      target, and then showed it gone after a click at its position.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3810
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gunslinger-10.hqx
    expected_sha256: 9142390326e35594d2a79160c46ec11d7c7b6c23be32709a9e0ab609583ca0d4
    expected_size: 764501
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gunslinger-10.hqx
    rights_holder: David McWherter Enterprises
    permission: >-
      The bundled author Read Me permits distribution of Gunslinger 1.0 when
      its application, Read Me, and GunslingerRES file remain together and
      unaltered. It forbids profit from CD-ROM or other bulk distribution.
      This entry preserves the complete original package for free online play.
    notes: >-
      Original 764,501-byte BinHex/StuffIt archive, SHA-256
      9142390326e35594d2a79160c46ec11d7c7b6c23be32709a9e0ab609583ca0d4.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/gunslinger/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3810
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only crop of the release browser game's active scene and target;
      Classic Mac menu bar and emulator framing are excluded.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gunslinger-10.hqx
---

## Pick your targets

![Gunslinger shootout scene with a target](incoming/gunslinger/gameplay.png)

Choose **File → New Game** to begin. Click armed targets in the photographed
scene while leaving unarmed people alone. The original freeware package
includes its required `GunslingerRES` picture file.
