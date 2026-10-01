---
id: tang-chi-demo
kind: game
title: Tang Chi Demo
summary: Arrange tangram pieces to match illustrated silhouettes.
developer: Foley Hi-Tech Systems
publisher: Capcom
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, selected File > New, entered the first
      tangram puzzle, and dragged a triangular piece from the tray onto the board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3682
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      In Chrome, fetched the unchanged archive once, selected File > New, entered
      the first tangram puzzle, and dragged a triangular piece onto the board. At
      10 MHz, a five-second active-play sample measured 60.0 host frames/s, 60.2
      guest ticks/s, 13.6 ms maximum frame, and 141 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3682
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/tang-chi-demo/archive.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/tang-chi
    rights_holder: Capcom / Foley Hi-Tech Systems
    permission: >-
      The unchanged archive contains the original Tang Chi demo and its README.TXT,
      which explicitly identifies it as a demo. No bundled redistribution restriction
      was found; no retail application is included.
    notes: >-
      Original 7,441,488-byte StuffIt archive, SHA-256
      fe074c686b7fa61e9b33437a05f12106ac5a560cbe98d1a5a9c2f72a3cd59497.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/tang-chi-demo/screenshot.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3682
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
references:
- https://classicmacdemos.com/tang-chi
---

## Arrange the pieces

![Tang Chi puzzle](incoming/tang-chi-demo/screenshot.png)

Choose File > New, click OK when the first level appears, then drag the
wooden pieces from the tray to match the silhouette on the board.
