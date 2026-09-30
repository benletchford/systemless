---
id: shell-game-demo
kind: game
title: Shell Game Demo
summary: Slide tiles and deduce which ones hide coins.
developer: BugByte Inc.
publisher: BugByte Inc.
year: 1993
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
    systemless_version: 87205416 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo with the generic
      document-viewer selection fix. The game board opened instead of the bundled order
      form; clicking the tile above the numbered space slid it into that space. One exact
      archive fetch. A five-second active-board sample measured 60.0 host FPS and 60.0
      guest ticks per second, maximum measured frame 0.4 ms, and minimum audio queue
      133 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3617
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo using its nested game application. Clicked
      through the launch screen to the board, then used the arrow tool on a tile beside
      the numbered empty space. The board visibly changed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3617
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 75c545953c0d685361b7ec39b8cc90ddce84ccca6fe1b415c4ea178ce2e68a05
    size_bytes: 235125
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/shell-game
    - https://static.classicmacdemos.com/demos/shell-game/README.txt
    rights_holder: BugByte Inc.
    permission: >-
      The original game's bundled program text says its locked form may be
      distributed and forbids distributing unlocked copies. This unchanged archive contains the
      locked purpose-built demo, its demo manual, readme, and order form; the demo
      manual describes ten levels rather than the full game's 100.
    notes: >-
      Unchanged 235,125-byte StuffIt archive, SHA-256
      75c545953c0d685361b7ec39b8cc90ddce84ccca6fe1b415c4ea178ce2e68a05.
      The promoted public asset was fetched back and matched this hash.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 65f39223d9dc77306484c4705031f15f7d07ddf56ba559c7027a649a776192ad
    size_bytes: 67011
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3617
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      408-by-313 direct Chrome capture of the board and tools after a tile slide. PNG
      SHA-256 65f39223d9dc77306484c4705031f15f7d07ddf56ba559c7027a649a776192ad, 67,011
      bytes.
      The promoted public asset was fetched back and matched this hash.
references:
- https://classicmacdemos.com/shell-game
- https://static.classicmacdemos.com/demos/shell-game/README.txt
---

## Find the coins

![Shell Game puzzle board](https://assets.systemless.org/catalogue/media/sha256/65/65f39223d9dc77306484c4705031f15f7d07ddf56ba559c7027a649a776192ad.png)

Click through the launch screen to start. Use the arrow tool to slide a tile
next to the empty space, then use the eye tool to check where coins are hidden.
The numbers reveal how many neighboring tiles conceal coins.
