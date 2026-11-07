---
id: macsokoban
kind: game
title: MacSokoban 3.0.2
summary: Push gold bags onto their destinations in the classic warehouse puzzle.
developer: Ingemar Ragnemalm
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original archive, explicit game executable and 68K slice,
      800x600/depth 8. The Players menu creates a new player. A first Up dismisses the level banner; a second Up and five Left moves approach a bag. A final Left pushes it one cell. The matched control omits only that final input and keeps the bag in place.
      Fresh asserted replays pass at 450 frontend / 1028 guest ticks, zero exhausted frames:
      six measured pixels and four exact prior-capture comparisons in both push and no-push replays. References are repeated Systemless captures,
      not independent hardware evidence. Native binary SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0.
      Production packing independently matches ten original paths, twenty forks and ten supported Finder records against
      unar extraction, with strict end-of-pack validation. Browser input/pacing and
      released/live qualification remain pending. Launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5707
runtime:
  executable_path: MacSokoban 3.0.2/MacSokoban 3.0.2
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/MacSokoban%203.0.2.sit
    expected_sha256: af0286028e29cb9a9c8b257dce9c75a68bbd3ae72379a29006d52f6771db82fe
    expected_size: 91557
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/MacSokoban%203.0.2.sit
    - https://github.com/benletchford/systemless/issues/5707
    license: Original noncommercial distribution grant
    rights_holder: Ingemar Ragnemalm
    permission: >-
      Topic 9 of the original in-application TEXT 0 help explicitly permits personal use and noncommercial distribution. The program is copyright Ingemar Ragnemalm 1992-1995; the notice distinguishes level ownership and requests a product copy for commercial distribution. Preserve the whole original package, documentation and bundled level-module attributions.
    notes: >-
      The actual game resource fork is 256413 bytes and contains CODE 0-3, with no cfrg. The bundled score mover is not the game executable.
      Exact original distribution, not repacked or converted.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/macsokoban/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5707
    permission: Original gameplay capture from the permitted unchanged game package.
    notes: >-
      Native candidate 357f79d1. 360x207 board crop at (220,151), without resampling; excludes the menu bar, desktop, title bar and window frame.
references:
- https://github.com/benletchford/systemless/issues/5707
---

![MacSokoban 3.0.2 during active gameplay](incoming/macsokoban/gameplay.png)

Choose **Add a new player** from **Players**, enter a name and accept. Dismiss the level banner, then use arrow keys or click adjacent squares to move and push bags. **Edit** offers undo and restart.

The bounded native replay verifies movement and a bag push, with an omitted-push control. Completed levels, external modules, saved progress and audio fidelity remain unverified.

Launch stays disabled pending hosted-original browser qualification and final source review.
