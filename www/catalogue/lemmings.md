---
id: lemmings
kind: game
title: Lemmings
summary: Guide the crowd through the four levels in Psygnosis's playable Macintosh mini-game demonstration.
developer: DMA Design
publisher: Psygnosis
year: 1992
architectures: [68k]
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-22
    tester: Catalogue maintainer
    systemless_version: 0.48.0
    architecture: 68k
    environment: >-
      Deterministic first-level run from the unchanged original StuffIt demo
      archive, cross-checked through the same script under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: c3e51e74044cdc15d82b5467d700dcdddae1d147ea9cc3dfb194d58fe82caa07
    size_bytes: 595722
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/lemmings
    - https://static.classicmacdemos.com/demos/lemmings/README.txt
    - https://www.playstation.com/en-gb/legal/copyright-and-trademark-notice/
    license: Psygnosis Lemmings promotional demo distribution
    rights_holder: Sony Interactive Entertainment Europe Limited
    permission: >-
      Psygnosis deliberately released this self-contained package as
      “Lemmings: The Demo Disk.” Its bundled Read Me and opening screen both
      describe it as a four-level mini-game, contrast it with the 120-level
      retail product and provide ordering details. That documented promotional
      distribution supports preservation of this exact unchanged demo; it does
      not extend to the retail game, modified copies or later releases.
    notes: >-
      The unchanged 595,722-byte StuffIt archive has SHA-256
      c3e51e74044cdc15d82b5467d700dcdddae1d147ea9cc3dfb194d58fe82caa07.
      It contains the 68K Lemmings application, four-level data, colour and
      black-and-white graphics, music, and Psygnosis's original Read Me. The
      package identifies itself as copyright 1992 Psygnosis Ltd.; Psygnosis
      later became part of Sony's PlayStation organisation. The catalogue hosts
      only this historical promotional demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 84fd7a6e11c6eef350b161e32419d8b693bfc75ded779594f7f90510086c9929
    size_bytes: 45220
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2222
    permission: >-
      Original gameplay screenshot captured for this catalogue at the
      maintainer's request. Underlying game artwork remains the property of
      Sony Interactive Entertainment Europe Limited.
    notes: >-
      Fresh deterministic Systemless 0.48.0 capture made from the exact
      unchanged demo archive on 2026-09-22 during the first playable level.
      The 800x600 framebuffer was cropped to the 640x400 game content surface,
      excluding the Mac desktop and menu bar; no game pixels were altered. PNG
      SHA-256
      84fd7a6e11c6eef350b161e32419d8b693bfc75ded779594f7f90510086c9929,
      45,220 bytes.
references:
- https://classicmacdemos.com/lemmings
- https://static.classicmacdemos.com/demos/lemmings/README.txt
- https://www.playstation.com/en-gb/legal/copyright-and-trademark-notice/
---

## Save the crowd

![Lemmings gameplay](https://assets.systemless.org/catalogue/media/sha256/84/84fd7a6e11c6eef350b161e32419d8b693bfc75ded779594f7f90510086c9929.png)

Fifty Lemmings pour from the trapdoor in the first demonstration level, and at
least half must reach the exit. The landscape is impassable without help, so
the player turns individual walkers into climbers, floaters, blockers,
builders or diggers. Every assignment is limited, the clock keeps moving and
one poor choice can redirect the entire crowd into danger.

The Macintosh version presents that puzzle in a colourful 640-by-400 playfield
with the miniature level map and skill panel always in view. Psygnosis's demo
contains four bespoke levels, enough to teach the basic vocabulary and show
how quickly a simple rescue plan becomes a chain of timing decisions.

## Psygnosis's four-level mini-game

This is the original 1992 promotional demo, not a copy of the 120-level retail
game. Its bundled Read Me calls the package “The Demo Disk,” explains that the
player is receiving a four-level mini-game and gives contemporary ordering
details for the complete product. The application repeats that distinction on
its opening screen.

Systemless opens the unchanged StuffIt archive, loads its original graphics,
levels and music, passes through the promotional screens and reaches live
Level 1 play. The same deterministic sequence matched BasiliskII at 99.1%
overall perceptual parity. Browser startup and asset review confirmed the
promoted immutable archive and screenshot.
