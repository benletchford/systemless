---
id: strategic-leap-checkers-deluxe
kind: game
title: Strategic Leap / Checkers Deluxe Demo
summary: >-
  Play MacSoft's six-capture checkers demonstration against a friend or the
  computer.
developer: Varcon Systems Inc.
publisher: MacSoft
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-30"
    tester: Catalogue maintainer
    systemless_version: 03092b61 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome browser preview of the unchanged StuffIt demo. Chose Basic
      Setup, entered two player names, and reached the Checkers Deluxe board. Selecting
      a black checker and clicking a diagonal square moved it and cleared its old
      square. One exact archive fetch. A 10-second active-board sample measured 60.0 host
      FPS, 60.2 guest ticks per second, maximum measured frame 2.8 ms, and minimum audio
      queue 131 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3577
  - date: "2026-09-30"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo at 800 by 600. Reached the Checkers Deluxe
      board and moved a red checker to a new square; the original square cleared and
      the game played its slide sound.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3577
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 628df9cc8876036f1a0ed65dda10e2445f29e4f64b0e1723db8c23175cb5c95e
    size_bytes: 370673
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/strategic-leap-checkers-deluxe
    - >-
      https://static.classicmacdemos.com/demos/strategic-leap-checkers-deluxe/README.txt
    rights_holder: Majestic Software
    permission: >-
      The unchanged archive contains MacSoft's promotional Checkers Deluxe demo, its
      bundled Read Me, and board setups. The Read Me documents the demo's disabled
      save/load and board editing features and its six-capture limit; it states no
      redistribution restriction. No retail game files are included.
    notes: >-
      Original 370,673-byte StuffIt archive, SHA-256
      628df9cc8876036f1a0ed65dda10e2445f29e4f64b0e1723db8c23175cb5c95e. The launch screen credits Varcon Systems as
      producer, Majestic Software as copyright holder, and MacSoft as publisher.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0f66383c7e7c4b0fb93be0112938d471527967e61ca1844eb8650c22043131f9
    size_bytes: 15128
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3577
    permission: >-
      Fresh gameplay capture made for this catalogue entry from the unchanged
      promotional demo. Underlying game artwork remains its owners' property.
    notes: >-
      565-by-393 browser capture of the Checkers Deluxe game window after a checker
      move. The crop excludes website framing, Mac desktop, and menu bar. PNG SHA-256
      0f66383c7e7c4b0fb93be0112938d471527967e61ca1844eb8650c22043131f9, 15,128 bytes.
references:
- https://classicmacdemos.com/strategic-leap-checkers-deluxe
- >-
  https://static.classicmacdemos.com/demos/strategic-leap-checkers-deluxe/README.txt
---

## Checkers with a twist

![Checkers Deluxe board after a checker move](https://assets.systemless.org/catalogue/media/sha256/0f/0f66383c7e7c4b0fb93be0112938d471527967e61ca1844eb8650c22043131f9.png)

The original demo appears as **Checkers Deluxe** when launched. Choose a
board and play against a friend or the computer. Select a checker, then click a
diagonal square to move it. Its bundled Read Me says
games stop after six pieces are removed; saving, loading, and board editing
are disabled. This catalogue entry preserves that original limited demonstration.
