---
id: xenia
kind: game
title: Xenia 1.0
summary: Pilot a fighter through a vertically scrolling arcade shooter.
developer: Richard Theil
publisher: Neon Skyline
year: 1997
architectures:
- 68k
- ppc
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
      The unchanged Xenia 1.0 archive reached the title, setup, Level 1, and active
      playfield in 68K Systemless. Shift started the game and fired a visible shot
      with a matching cannon indicator change. The release browser build fetched
      the same archive once, reached active play, fired a visible projectile
      with Shift, and moved the ship right after assigning the arrow keys in setup.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3801
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/xenia-10.hqx
    expected_sha256: 6b56d664748d5a5fbcfbe0dc97cc2bd65e92c10c718dfc424ab60419fa0775c4
    expected_size: 307683
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/xenia-10.hqx
    rights_holder: Richard Theil
    permission: >-
      The bundled author-supplied Read Me permits free downloading of the complete,
      unmodified package without charge. This entry retains that original package.
    notes: >-
      Original 307,683-byte BinHex package, SHA-256
      6b56d664748d5a5fbcfbe0dc97cc2bd65e92c10c718dfc424ab60419fa0775c4.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/xenia/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3801
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Cropped active Level 1 playfield with a ground turret and fired projectile
      from the release browser build.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/xenia-10.hqx
---

## Defend the skies

![Xenia Level 1](incoming/xenia/gameplay.png)

Press **Shift** on the title screen to begin. Use **Escape** to open setup,
where you can assign movement keys and change the fire and bomb controls.
Assign the arrow keys there for movement; **Shift** fires and **Command** drops bombs.
The original package includes the game, its data folder, and full instructions.
