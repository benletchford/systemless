---
id: smack-a-skunk
kind: game
title: Smack a Skunk 1.0
summary: Aim a perspective hammer at pop-up nuisances before ten escape.
developer: Ingemar Ragnemalm, Folke Söderström and Susanne Ragnemalm
publisher: Ingemar Ragnemalm
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
    systemless_version: 0.72.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive launched in 68K Systemless. A mouse
      click on the title screen started a live board. Moving the pointer
      positioned the perspective hammer, and a click on a visible pop-up
      raised the displayed score from 0 to 1,000 megafantazillions while
      the missed count stayed at zero. The release browser build fetched the
      same archive once, reached the live board, showed moving pop-ups, and
      moved the hammer with mouse input. A browser score hit was not confirmed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3782
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/smack-a-skunk-10.hqx
    expected_sha256: 27290e8a58e87ab1a4b2a2a90de04745927c835cde417acaa2caac94f45e990b
    expected_size: 444347
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/smack-a-skunk-10.hqx
    rights_holder: Ingemar Ragnemalm, Folke Söderström and Susanne Ragnemalm
    permission: >-
      The bundled Smack a Skunk docs call the game freeware for personal
      use and expressly allow unmodified copies to be freely distributed.
      This entry preserves the complete, unchanged archive and docs.
    notes: >-
      Original 444,347-byte Info-Mac BinHex/StuffIt archive, SHA-256
      27290e8a58e87ab1a4b2a2a90de04745927c835cde417acaa2caac94f45e990b.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/smack-a-skunk/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3782
    permission: >-
      Fresh Systemless gameplay capture from the unchanged free edition.
      Underlying artwork remains its owners' property.
    notes: >-
      Exact 636-by-480 game-content crop at (82,60) from an 800-by-600
      Systemless framebuffer just after a successful hit; displayed score
      1,000 megafantazillions and another pop-up visible.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/smack-a-skunk-10.hqx
---

## Aim the hammer

![Smack a Skunk board after a successful hit](incoming/smack-a-skunk/gameplay.png)

Click the title screen to start. Move the mouse to place the hammer over a
pop-up, then click to smack it. The round ends after ten pop-ups escape.
