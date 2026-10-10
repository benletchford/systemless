---
id: sigma-chess
kind: game
title: Sigma Chess
summary: >-
  Play classic chess against the computer with a colourful board, move record
  and a library of historical games.
developer: Ole Kjær Christensen and Kaare Danielsen
publisher: Christensen and Danielsen
year: 1998
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 1b471bc265a230610f61b33fdc8edd0c78693c3f
    architecture: 68k
    status: playable
    environment: >-
      Bounded native frontend-tick replay of the intact 164-file Lite 4.0 archive,
      exact main game selected, default 8-bit display. Drag e2-e4 and d2-d4;
      computer replies e7-e6 and d7-d5 and records both turns. Actual captures
      inspected. Repeat passes four measured board-pixel assertions at 912 frontend
      ticks with zero budget exhaustion. Browser, complete matches, sustained search, other modes,
      auxiliary generators, saving and audio remain unverified.
    evidence: https://github.com/benletchford/systemless/issues/4418
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 5e82d0a34a7527fe7a4ef75b4f9bf37ab92dc1d2
    architecture: 68k
    status: playable
    environment: >-
      Optimized local release preview in ordinary Chrome with isolated worker
      and WebGL. Original archive bytes supplied to the preview request after
      independent SHA-256 and size verification. Drag e2-e4 and d2-d4;
      computer replies e7-e6 and d7-d5 and records both turns. Per-step
      captures inspected. Public route, complete matches, sustained search,
      other modes, auxiliary generators, saving and audio remain unverified.
    evidence: https://github.com/benletchford/systemless/issues/4418
runtime:
  executable_path: Sigma Chess Lite 4.0/∑ Chess 4.0 Lite
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://github.com/benletchford/systemless/releases/download/catalogue-intake-20261010/sigma-chess-lite-40.sit
    expected_sha256: 1ec34342042511ef0accdaea957eeb174ba0b2955d7ea53c8ab3113d110bc11a
    expected_size: 925961
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/s/
    - https://www.vintageapplemac.com/files/games/Sigma%20Chess%20Lite%204.0.sit
    - https://github.com/benletchford/systemless/issues/4418
    license: Original Sigma Chess 4.0 Lite freeware distribution notice
    rights_holder: Ole Kjær Christensen and Kaare Danielsen
    permission: >-
      The original Read me! dated April 29, 1998 expressly permits free
      distribution of Lite 4.0 in its original form and requests that all
      sample games, collections, libraries and endgames be retained. The
      main executable also states that Lite is freely distributable and
      may not be modified. The complete original package is preserved.
    notes: >-
      Intact 164-file distribution including main game, online manual,
      sample games, collections, opening library and original 68K/PPC
      endgame generators. No registration key or bypass. Main game has
      zero-byte data fork, nine CODE resources and no cfrg/PPC slice.
      PPC generators are auxiliary applications, not a PPC game port.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/sigma-chess/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4418
    permission: Original Systemless gameplay capture requested by the maintainer.
    notes: >-
      Actual Lite 4.0 board and move record after two legal player moves
      and computer replies, cropped to game content without the Classic
      menu bar, window title or host UI.
references:
- https://www.vintageapplemac.com/software/games/s/
---

![Sigma Chess gameplay](incoming/sigma-chess/gameplay.png)

Play the original **Sigma Chess 4.0 Lite** freeware distribution. Drag a white
piece to a legal square; the computer responds as black. The board starts at
five minutes per player. **Analyze → Stop** interrupts a search and plays the
best move found. Use **File → New Game** when it is your turn to start again.

Bounded native verification covers **e2–e4**, **d2–d4**, and the computer replies
**e7–e6**, **d7–d5** with the move record updated. Optimized browser verification also passes both player moves and computer replies.
Public release verification is pending.
Complete matches, sustained search, other modes, saving and audio remain
unverified. The original Lite edition retains its feature limits, including
unsavable libraries and limited transposition tables.

This original main game is **68K only**. The included PPC endgame generators
are auxiliary tools and are not counted as a second game or a PPC game port.
The complete original archive retains all 164 files and distribution notices.
