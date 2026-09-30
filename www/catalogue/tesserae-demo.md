---
id: tesserae-demo
kind: game
title: Tesserae 1.01 Demo
summary: Clear a colorful mosaic by jumping and combining tiles.
developer: Nicholas J. Schlott
publisher: Inline Design
year: 1990
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
    systemless_version: fbce8d9e + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo. Dismissed the
      startup notice, chose Start and Mosaic 1, and dismissed the dealer information dialog.
      Two browser tile clicks reduced the board from 48 to 46 tiles and advanced the
      move counter to 1. One exact archive fetch. A 10-second active-board sample
      measured 60.0 host FPS, 60.1 guest ticks per second, maximum measured frame 11.8 ms, and
      minimum audio queue 132 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3606
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo at 800 by 600. Dismissed startup, chose
      Start and Mosaic 1, then dismissed the dealer information dialog. Selecting and
      moving a tile reduced the board from 48 to 46 tiles and advanced the move counter
      from 0 to 1.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3606
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 13750a01b0393b9f25f624f78baa592941f2d712cc246e5918119868d62d41d4
    size_bytes: 226349
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/tesserae
    - https://www.macintoshrepository.org/19424-the-macintosh-demo-games-cd
    rights_holder: Nicholas J. Schlott / Inline Design
    permission: >-
      The demo's startup notice permits free distribution of the unchanged demo and
      requires the notice to remain unchanged. It forbids selling the demo, except that
      nonprofit Macintosh user groups may charge only for the media. This unchanged
      archive contains the purpose-built 1.01 demo and its distributor list, not the
      retail game. A contemporary Macintosh Demo Games CD also lists Tesserae among its
      included demos.
    notes: >-
      Unchanged 226,349-byte StuffIt archive, SHA-256
      13750a01b0393b9f25f624f78baa592941f2d712cc246e5918119868d62d41d4.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: bd9120cba1b2bd11b17b768f5d46cc1dee98576b9aa49764480fe551caa8cc89
    size_bytes: 57351
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3606
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      642-by-461 direct Chrome capture of Mosaic 1 after the first completed tile
      move. The crop excludes website framing, Mac desktop, menu bar, and window frame.
      PNG SHA-256 bd9120cba1b2bd11b17b768f5d46cc1dee98576b9aa49764480fe551caa8cc89,
      57,351 bytes.
references:
- https://classicmacdemos.com/tesserae
- https://www.macintoshrepository.org/19424-the-macintosh-demo-games-cd
---

## Clear the mosaic

![Tesserae mosaic board](https://assets.systemless.org/catalogue/media/sha256/bd/bd9120cba1b2bd11b17b768f5d46cc1dee98576b9aa49764480fe551caa8cc89.png)

Choose **Start**, accept the first mosaic, and dismiss the dealer information
dialog. Select a tile, then click a highlighted destination to jump it across
the board and clear matching pieces.
