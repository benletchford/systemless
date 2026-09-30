---
id: checkers-deluxe-demo
kind: game
title: Checkers Deluxe Demo
summary: Play a limited checkers match against a friend or the computer.
developer: Bob Garner
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
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, started a two-person match, reached the
      board, and moved a checker diagonally into an empty square.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3640
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + release browser
    architecture: 68k
    environment: >-
      Fetched the exact archive once in a release-mode browser, started a Basic Setup
      match, and moved black and red checkers into open diagonal squares. A
      five-second active-board sample measured 60.0 host FPS and 60.2 guest ticks/sec, with a 1.7
      ms maximum measured frame and 131 ms minimum audio queue. The 554-by-353
      gameplay screenshot is a direct capture of the game board and player panels.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3640
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
    - https://download.classicmacdemos.com/Strategic%20Leap.sit
    rights_holder: Varcon Systems / Majestic Software / MacSoft
    permission: >-
      The unchanged archive includes a purpose-built Checkers Deluxe demo and a
      ReadMe explaining its game limits. No bundled redistribution restriction was found. No
      retail application is included. The splash identifies Varcon Systems, Majestic
      Software, and MacSoft.
    notes: >-
      Original 370,673-byte StuffIt archive, SHA-256
      628df9cc8876036f1a0ed65dda10e2445f29e4f64b0e1723db8c23175cb5c95e.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3f906a800a4aead7741485ecd75659deea339f8cb076bfe1a0be88e35c23db1e
    size_bytes: 12484
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3640
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
    notes: >-
      Direct 554-by-353 release-browser capture of the active board and player
      panels, excluding website framing, the Mac menu bar, and the window frame.
references:
- https://www.macintoshrepository.org/5342-checkers-deluxe
---

## Play a match

![Checkers Deluxe game board](https://assets.systemless.org/catalogue/media/sha256/3f/3f906a800a4aead7741485ecd75659deea339f8cb076bfe1a0be88e35c23db1e.png)

Choose New Game, select a board, and enter the players' names. Click a checker,
then click an open diagonal square to move it. The demo ends a game after six
pieces have been removed.
