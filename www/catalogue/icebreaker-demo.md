---
id: icebreaker-demo
kind: game
title: Icebreaker Demo
summary: >-
  Smash pyramids by luring them into each other and into hazards in Magnet
  Interactive's colourful action puzzle game.
developer: Magnet Interactive Studios
publisher: Magnet Interactive Studios
year: 1995
architectures:
- ppc
default_architecture: ppc
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged demo archive ran natively on PowerPC. Level Grid, level 1 and
      Let's Go started play. Holding keypad 6 and then keypad 2 moved the player and
      scrolled the map. A matched idle control was identical until the input and then
      differed by 275,245 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. The demo binds movement to the numeric keypad, not the arrow keys.
      Later levels, audio remain unverified. Browser pacing held 60 fps but every sample
      had one 66–68 ms frame, so launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5504
controls:
  arrows_as_numpad: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 4997d7b4bef41317bd307c67ef10ed0b5161b8bf282a13c127d836fa7b79887d
    size_bytes: 5257721
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/icebreaker
    - https://download.classicmacdemos.com/Icebreaker%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5504
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Magnet Interactive Studios
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: >-
      PowerPC demo; Random Level is disabled in this version, but the level grid and
      tutorials are playable.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f1b0f6dc166747ef96d124536304ce1f6f8c3500ec38002cd46d1861feb6247e
    size_bytes: 85019
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5504
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native PowerPC level 1 play, cropped to the game window.
references:
- https://classicmacdemos.com/icebreaker
- https://github.com/benletchford/systemless/issues/5504
---

## Break the ice

![Icebreaker Demo](https://assets.systemless.org/catalogue/media/sha256/f1/f1b0f6dc166747ef96d124536304ce1f6f8c3500ec38002cd46d1861feb6247e.png)

Every level is a field of coloured pyramids. Some you can shatter directly, others must be tricked into hazards or lured by the seekers that chase you.

Steer with the arrow keys or the numeric keypad (including diagonals). Press P to pause.
