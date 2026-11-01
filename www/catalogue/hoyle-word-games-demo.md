---
id: hoyle-word-games-demo
kind: game
title: Hoyle Word Games Demo
summary: >-
  Solve crosswords and other word puzzles in Sierra's Hoyle collection, with
  clue lists, hints and a newspaper-style grid.
developer: Sierra On-Line
publisher: Sierra On-Line
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged demo archive ran natively on PowerPC and opened a Crosswords puzzle
      from the game hub. Typing ARCH filled 1-Across. A matched idle control was identical
      until the input and then differed by 13,203 pixels. An exact replay matched every
      checkpoint, with zero exhausted frames. The other word games, audio remain unverified.
      Browser pacing in the crossword passed three of four samples at about 60 host fps and
      60 guest ticks/s; the fourth had one 51.1 ms frame.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5497
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Hoyle%20Word%20Games%20Demo.sit
    expected_sha256: 5457f1c4b3ec355305ee69a01b8428e44984520dffb12d5a05eb69f22b5a8f7d
    expected_size: 18867363
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/hoyle-word-games
    - https://download.classicmacdemos.com/Hoyle%20Word%20Games%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5497
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Sierra On-Line
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: >-
      PowerPC demo of Sierra's word-game collection. Only Crosswords and a few other
      games are playable in the demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/hoyle-word-games-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5497
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game artwork
      remains its owners' property.
    notes: Native PowerPC Crosswords puzzle, cropped to the game window.
references:
- https://classicmacdemos.com/hoyle-word-games
- https://github.com/benletchford/systemless/issues/5497
---

## Crossword time

![Hoyle Word Games Demo](incoming/hoyle-word-games-demo/gameplay.png)

Pick a word game from the hub and play. The crossword offers full Across and Down clue lists and hints, and lets you reveal a letter, word or whole puzzle.

Click a clue or square, then type your answer. Press the spacebar to switch between Across and Down.
