---
id: macigame
kind: game
title: MaciGame
summary: Remove connected groups of matching characters and clear the board for a higher score.
developer: Takeshi Takahashi / Ayako Takahashi / Miya Takahashi
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: d02a603f native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original package and exact macigame1.60 /MaciGame v1.60E
      executable, including the original folder's trailing space. Explicit 68K, default
      25 MHz/800x600/depth 8 and original partition. First click at (624,72) selects a
      visible three-green-piece group, showing Get:3/Points:1. Second click removes it,
      leaving Rest:237/Score:1. Equal-clock omitted-button control retains Rest:240/Score:0.
      Each fresh repeat passes six measured causal pixels and three exact prior screenshots
      at frontend/guest tick 308/908, with zero exhaustion. Actual qualified images inspected;
      own-run references establish consistency, not hardware accuracy. Native binary SHA-256
      d961445d3cbcee316a041419a75a24eaaaf6d0be22bf33ec3d8a4e1a8ac73b5e. No game, seed,
      clock, depth, character-set or partition changes. Full games, other character sets,
      column-collapse, persistence, audio fidelity, browser pacing and released public play
      remain unverified. Launch stays disabled pending browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5837
runtime:
  executable_path: macigame1.60 /MaciGame v1.60E
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mac/info-mac/game/arc/maci-game-160.hqx
    expected_sha256: 93bb0e7c9941a0682a8b429f46aac0a359c19efb3e21d89b8622c61751dee4a5
    expected_size: 106724
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/maci-game-160.hqx
    - https://github.com/benletchford/systemless/issues/5837
    license: Original free unchanged-distribution grant; no commercial use
    rights_holder: Takeshi Takahashi; retained original contributors
    permission: >-
      The complete original ReadMe permits playing and distributing without charge,
      requires documentation to accompany the game, prohibits modified copies and
      commercial use, and requests advance notification for printed/floppy/CD publication.
      Preserve the notice, original Score file, all character sets, forks and credits.
    notes: >-
      Full 5388-byte ReadMe reviewed. Copyright 1994 Takeshi Takahashi; co-produced by
      Ayako and Miya, with original game-idea and Neko Neko/Youkan character credits.
      The English 1.60E executable is already present in this original package; no
      translation patch is applied. All six supplied character sets form one game intake,
      not separate variants. Independent packing matches nine files, eighteen forks and
      nine Finder records. Strict KPK end 192197, SHA-256
      f41df44d5d150df21ea4a295ebaa30618cc304a4e4954ba861ac098455a58217.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/macigame/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5837
    permission: Original gameplay capture of the unchanged freely distributable game.
    notes: >-
      Three pieces removed, Rest:237/Score:1. Native crop (0,40)-(640,440), 640x400,
      without resampling; excludes menu and host interface.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/maci-game-160.hqx
- https://github.com/benletchford/systemless/issues/5837
---

## Match the characters

![Three green pieces removed for one point](incoming/macigame/gameplay.png)

Click a connected group of matching characters to select it, then click again to
remove it. Larger groups earn more points. Pieces fall into gaps, and empty
columns close from the right. The original documentation and character sets
accompany the game download.
