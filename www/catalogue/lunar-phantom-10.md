---
id: lunar-phantom-10
kind: game
title: Lunar Phantom 1.0
summary: >-
  Pilot a small spacecraft through a black-and-white lunar landscape,
  managing rotation and thrust to survive its obstacles.
developer: Rolf Staflin
publisher: Rolf Staflin
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
      The unchanged original shareware package opened in 68K Systemless.
      Game > New Game reached an active level with the bonus timer advancing.
      At the same guest tick, the documented Z key rotated the ship while
      the idle run left it level. The release browser fetched the archive
      once and reproduced the level and rotation control.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3881
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/lunar-phantom-10.hqx
    expected_sha256: ea6b0d0eb089ce9c018654169b91bb798a2f8fdfb76ca6b459fb19784f45a70f
    expected_size: 343252
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/lunar-phantom-10.hqx
    rights_holder: Rolf Staflin
    permission: >-
      The bundled About Lunar Phantom 1.0 document explicitly permits
      sharing copies with friends and uploading the game to a BBS or
      internet archive. It asks commercial CD-ROM distributors to register.
      This is the unchanged original shareware package with that document.
    notes: >-
      Original 343,252-byte Info-Mac BinHex/StuffIt package; SHA-256
      ea6b0d0eb089ce9c018654169b91bb798a2f8fdfb76ca6b459fb19784f45a70f.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/lunar-phantom-10/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3881
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owner's property.
    notes: >-
      515-by-325 game-content crop at (127,148) from an 800-by-600 Systemless
      framebuffer on the first level after Z rotated the spacecraft.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/lunar-phantom-10.hqx
---

## Fly the lunar course

![Lunar Phantom first level with the spacecraft tilted](incoming/lunar-phantom-10/gameplay.png)

Choose **New Game** from the **Game** menu. Press **Z** to rotate
counterclockwise, **X** to rotate clockwise, and **Period** to thrust.
The **Instructions** item in the Game menu explains the controls and goal.
