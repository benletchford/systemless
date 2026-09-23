---
id: simearth
kind: game
title: SimEarth
summary: Explore a changing planet in Maxis's limited SimEarth Explorer Color demo.
developer: Maxis
publisher: Maxis
year: 1990
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.51.0"
    architecture: 68k
    environment: >-
      Deterministic headless run from the unchanged Macintosh StuffIt demo through
      the Explorer introduction and Earth scenario map
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2527
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 2a935c94905ab01d398eb00528689b746b04d1224f992208d2aaf46afa63deee
    size_bytes: 263889
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/simearth
    - https://www.macintoshrepository.org/19424-the-macintosh-demo-games-cd
    license: Maxis SimEarth Explorer Color promotional demo
    rights_holder: Electronic Arts Inc.
    permission: >-
      This is the separate, limited Maxis Explorer Color demonstration, not the
      commercial SimEarth game. The included Description presents it as a quick overview,
      and SimEarth was distributed among the demos on Apple's 1992 Macintosh Demo Games
      CD. The preserved package contains no notice prohibiting no-charge redistribution
      of this unchanged demo; this basis does not extend to retail editions,
      registration data, or modifications.
    notes: >-
      The unchanged 263,889-byte StuffIt archive has SHA-256
      2a935c94905ab01d398eb00528689b746b04d1224f992208d2aaf46afa63deee. It contains SimEarth Explorer Color and
      its Description, not a full game.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 74d971458c46fbc6e4c41ded51a68ba645ff291caac41ad7692613bbd2eb06f4
    size_bytes: 28085
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2527
    permission: >-
      Fresh gameplay screenshot captured for this catalogue from the original demo.
      Underlying SimEarth artwork remains the property of its owner.
    notes: >-
      Deterministic Systemless Earth-scenario capture, cropped from the 800x600 guest
      framebuffer to the 514x303 game content surface. It excludes the Mac menu bar,
      title and desktop, and alters no game pixels. PNG SHA-256
      74d971458c46fbc6e4c41ded51a68ba645ff291caac41ad7692613bbd2eb06f4; 28,085 bytes.
references:
- https://macintoshgarden.org/games/simearth
- https://www.macintoshrepository.org/19424-the-macintosh-demo-games-cd
---

## A planet in miniature

![SimEarth Explorer Color Earth scenario](https://assets.systemless.org/catalogue/media/sha256/74/74d971458c46fbc6e4c41ded51a68ba645ff291caac41ad7692613bbd2eb06f4.png)

Maxis's Explorer Color demo introduces the planet model through guided text
and a selectable Earth scenario. The map, world clock and control panels are
interactive, but this is a limited overview rather than the retail simulation.

This entry preserves the original demo archive without registration material
or a repack. Direct browser launch will remain disabled until the promoted
archive passes a graphical gameplay check in Systemless.
