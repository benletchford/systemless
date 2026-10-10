---
id: ingemars-skiing-game
kind: game
title: Ingemar's Skiing Game
summary: Steer through slalom gates in the original complete freeware skiing game.
developer: Ingemar Ragnemalm
year: 2000
architectures:
- 68k
default_architecture: 68k
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 1f9b99bc8afa3013ccbe7e99c84b460346bb58dc
    architecture: 68k
    environment: >-
      Bounded native replay from the complete unchanged original-author 1.0.3 BinHex
      archive. Explicit original game executable, default 800-by-600 display and 8-bit
      depth. Game > Practice 1 and an ordinary mouse click start a race; mouse steering
      moves the visible skier left and right as the timer and course advance.
      Inspected captures and a fresh six-assertion repeat pass at 1138 frontend / 1714 guest
      ticks with zero exhausted budgets. Full courses, cups, editor, saves and audio
      remain unverified. Main and optional launcher contain CODE resources and no PPC cfrg;
      this original package is 68K-only.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4444
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 9bbeb477e4ff244c4370be5d40fe816fd979eeb2
    architecture: 68k
    environment: >-
      v0.92.2 ordinary Chrome worker/WebGL preview with cross-origin isolation,
      complete production-decoded nested executable path and 800-by-600/8-bit display.
      Game > Practice 1 and an ordinary click start the race. A fresh normal mouse
      replay visibly moves the skier right then left while the timer and course
      advance; captures inspected. Local-origin CORS requires only an unchanged R2
      archive fixture independently verified against the promoted SHA-256 and size.
      All four original files and both forks match independent unar extraction
      byte-for-byte. Public replay, full courses, cups, editor, saves and audio
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4444
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: ec812923f4ac82d91745bc03e3770eb23c9149e8
    architecture: 68k
    environment: >-
      Actual public systemless.org route on v0.94.0, ordinary Chrome worker/WebGL
      runtime, 800-by-600 guest canvas and 25 MHz pacing. Game > Practice 1 and
      a normal click start the race. Right then left mouse input visibly steers
      the skier while the course and timer advance; actual captures inspected.
      Passive actual player archive response is HTTP 200 with original SHA-256
      and 309448-byte size verified, without interception or preview fixtures.
      Public crossOriginIsolated is false. All six native release platforms and
      their twelve published archive/checksum assets passed whole-pipeline acceptance.
      Full courses, cups, editor, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4444
runtime:
  executable_path: Ingemar's skiing game 1.0.3.cpt/Ingemar's Skiing Game 1.0.3 ƒ/Ingemar's skiing game 1.0.3
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: baefe3381b5dc05b929c70b59b2189959213dbea07310560b3feb193750d495c
    size_bytes: 309448
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.lysator.liu.se/~ingemar/games.html
    - https://www.lysator.liu.se/~ingemar/games/news.html
    - https://ftp.lysator.liu.se/pub/mac/games/ISG-103.hqx
    - https://github.com/benletchford/systemless/issues/4444
    license: Original freeware distribution grant
    rights_holder: Ingemar Ragnemalm
    permission: >-
      The original game's TEXT 128 legal terms explicitly permit free distribution
      provided charges do not exceed distribution costs. The author independently
      declares ISG freeware with no demand for payment and no distribution restrictions on his
      games news page. This complete unchanged freeware 1.0.3 distribution retains its
      original notices, game, optional launcher, Read me first and Easy courses.
    notes: "Original author's ISG-103.hqx, independently matched after intake upload. No repacking or fork conversion. Main has eight CODE resources and no PPC cfrg. The year describes this complete freeware release: original application and documentation modification dates are in August 2000, and the launcher notice is copyright 2000. The game originated earlier in the 1990s."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: dd4f38d0c716b13ed13b4e1cdff82eab7891f065ca45f47b7d1a07a6d91e1872
    size_bytes: 309875
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4444
    permission: Original Systemless gameplay capture requested by the maintainer.
    notes: >-
      Inspected unedited 800-by-600 native guest frame during the practice race after
      rightward steering, with skier, gates and advancing timer visible. Contains only
      the original game and guest desktop, with no host application UI.
references:
- https://www.lysator.liu.se/~ingemar/games.html
- https://www.lysator.liu.se/~ingemar/games/news.html
---

![Ingemar's Skiing Game practice race](https://assets.systemless.org/catalogue/media/sha256/dd/dd4f38d0c716b13ed13b4e1cdff82eab7891f065ca45f47b7d1a07a6d91e1872.png)

Race through slalom gates in the complete original **Ingemar's Skiing Game 1.0.3**
freeware release. Choose **Practice 1** from **Game**, then click to leave the
starting booth. Move the mouse left or right to steer. The original built-in
help describes keyboard controls, courses and tournament play.

The complete original distribution includes the optional launcher, documentation
and Easy courses. Its original freeware notice permits distribution without
charges beyond distribution costs. This package contains a 68K game; no PowerPC
slice is present.

Bounded native testing covers starting a practice race, visible left/right
steering and advancing course/timer. Ordinary worker/WebGL browser testing also
covers practice start and right/left steering with the original archive.
Public-route practice start and right/left steering are verified on v0.94.0.
Full courses, cups, editing, saves and audio remain unverified.
