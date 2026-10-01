---
id: prima-materia
kind: game
title: Prima Materia v0.1.1
summary: Clear colorful brick fields with a mouse-controlled paddle.
developer: Scumby Software
publisher: Scumby Software
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
      The unchanged Prima Materia v0.1.1 archive reached its high-scores menu
      and active Level 1 brick board in 68K Systemless. Matched deterministic
      replays with the mouse at opposite sides placed the paddle at the
      corresponding left and right edges; the ball hit bricks and scored.
      The release browser build fetched the same archive once, reached Level 1,
      and visibly moved the paddle with mouse input.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3806
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/prima-materia-011.hqx
    expected_sha256: 31fb5c0f3169c24754f2d489ce3c50baa25a96abba7ceb253dfdb4b717869317
    expected_size: 1647444
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/prima-materia-011.hqx
    rights_holder: Scumby Software
    permission: >-
      The bundled author-supplied Read Me permits free copying of the unaltered
      beta package. It prohibits charged distribution, including a bundle with
      anything charged for. This entry retains the complete original package
      for free online access.
    notes: >-
      Original 1,647,444-byte BinHex package, SHA-256
      31fb5c0f3169c24754f2d489ce3c50baa25a96abba7ceb253dfdb4b717869317.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/prima-materia/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3806
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Cropped active Level 1 brick board, ball, and paddle from the release browser build.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/prima-materia-011.hqx
---

## Keep the ball in play

![Prima Materia Level 1](incoming/prima-materia/gameplay.png)

Select **play** from the high-scores menu. Move the mouse left and right to
position the paddle, bounce the ball through the bricks, and collect power-ups.
This is the original free beta release, with its Read Me and game data intact.
