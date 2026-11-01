---
id: the-tinies-demo
kind: game
title: The Tinies Demo
summary: >-
  Slide fuzzy Tinies around walled puzzle boards until each one rests on the pad
  that matches its colour.
developer: Atreid Concept
publisher: Inline Design
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice. Game > New Game drew
      level 1, and the arrow keys moved the selector and a Tiny. A matched idle control
      was identical until the input and then differed by 33,858 pixels. An exact
      replay matched both checkpoints, with zero exhausted frames. Later levels, audio
      remain unverified. Four browser samples in level 1 passed at 57–58 host fps and 58–60
      guest ticks/s at default pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5514
runtime:
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: abf374ff72b7a7f8578d2153e633a08373bde859f0e975087d38658def082c4e
    size_bytes: 322180
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/the-tinies
    - https://download.classicmacdemos.com/Tinies%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5514
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Atreid Concept
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: 68K demo of the colour-matching puzzle game known in Europe as Tiny Skweeks.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ea3d35524906125639b96448b6cd444844d1d5ca86dc8f59b8b67f0b0fa7325d
    size_bytes: 13300
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5514
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K level 1, cropped to the game window.
references:
- https://classicmacdemos.com/the-tinies
- https://github.com/benletchford/systemless/issues/5514
---

## Colour coded

![The Tinies Demo](https://assets.systemless.org/catalogue/media/sha256/ea/ea3d35524906125639b96448b6cd444844d1d5ca86dc8f59b8b67f0b0fa7325d.png)

Each board is a maze of walls, holes and obstacles. Push every Tiny onto the pad of its own colour before the clock runs out.

Choose Game > New Game, then use the arrow keys to pick a Tiny and slide it around the board.
