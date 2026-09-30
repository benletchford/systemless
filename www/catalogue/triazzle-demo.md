---
id: triazzle-demo
kind: game
title: Triazzle Demo
summary: Match rainforest creatures by fitting illustrated triangles into a puzzle.
developer: Berkeley Systems
publisher: Berkeley Systems
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
      Replayed the unchanged StuffIt demo, entered its one-level rainforest puzzle,
      observed the timer advance, and dragged a triangle onto the board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3648
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + release-mode browser build
    architecture: 68k
    environment: >-
      In Chrome, fetched the unchanged archive once, entered the rainforest
      puzzle, observed its timer advance, and dragged a triangle onto the board.
      At 10 MHz, a five-second active-play sample measured 60.0 host frames/s,
      60.2 guest ticks/s, 13.5 ms maximum frame, and 131 ms minimum audio queue.
      A thin outline from the How to Play dialog remains visible around the board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3648
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Triazzle%20Demo.sit
    expected_sha256: feae6c7b25a4e3053b0b5cad97e7068fe4064fa2910e909c77af02404df30197
    expected_size: 1285991
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/triazzle
    rights_holder: Berkeley Systems / Dan Gilbert
    permission: >-
      The unchanged archive contains Triazzle 1.0 Demo and its Read Me, which
      identifies this as a one-level limited release and promotes the full product.
      No bundled redistribution restriction was found; no retail application is included.
    notes: >-
      Original 1,285,991-byte StuffIt archive, SHA-256
      feae6c7b25a4e3053b0b5cad97e7068fe4064fa2910e909c77af02404df30197.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/triazzle-demo/screenshot.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3648
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
references:
- https://classicmacdemos.com/triazzle
---

## Match the creatures

![Triazzle rainforest puzzle](incoming/triazzle-demo/screenshot.png)

Press Return at the title and again to dismiss How to Play. Select a triangle,
then drag it onto the board. Click a piece to rotate it, or Option-click to
rotate in the other direction. Match every creature along the touching edges.
The How to Play dialog may leave a thin outline over the board.
