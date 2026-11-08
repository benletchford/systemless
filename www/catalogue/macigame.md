---
id: macigame
kind: game
title: MaciGame
summary: >-
  Remove connected groups of matching characters and clear the board for a higher
  score.
developer: Takeshi Takahashi / Ayako Takahashi / Miya Takahashi
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: d02a603f native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original package and exact macigame1.60 /MaciGame v1.60E
      executable, including the original folder's trailing space. Explicit 68K, default 25
      MHz/800x600/depth 8 and original partition. First click at (624,72) selects a
      visible three-green-piece group, showing Get:3/Points:1. Second click removes it,
      leaving Rest:237/Score:1. Equal-clock omitted-button control retains
      Rest:240/Score:0. Each fresh repeat passes six measured causal pixels and three exact prior
      screenshots at frontend/guest tick 308/908, with zero exhaustion. Actual qualified
      images inspected; own-run references establish consistency, not hardware
      accuracy. Native binary SHA-256
      d961445d3cbcee316a041419a75a24eaaaf6d0be22bf33ec3d8a4e1a8ac73b5e. No game, seed, clock, depth, character-set or partition changes. Full
      games, other character sets, column-collapse, persistence, audio fidelity, and released public play remain unverified. Hosted-original browser evidence follows below.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5837
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 46516ac4 private browser preview on d02a603f runtime
    architecture: 68k
    environment: >-
      Fresh isolated Chromium/Linux profiles, worker/WebGL and SharedArrayBuffer,
      default 25 MHz/800x600/depth 8, production COEP credentialless and COOP same-origin
      headers. Actual source stamp, architecture, worker and clock checked. Each unchanged
      20-second sample includes visible group detection from the rendered PNG and two
      selection/confirmation inputs, or omitted-button control. No guest-memory reader,
      game, seed, board, character-set, sound, clock or partition changes and no hidden
      warm-up gameplay. Active selects and removes four visible pieces, leaving
      Rest:236/Score:4; pointer-only control retains its own original board at
      Rest:240/Score:0. The fresh initial boards differ naturally; this is not an
      identical-board comparison. Actual before/selection/after and control images
      inspected; the control board's rendered class map remains unchanged. Actions finish
      at 3.766/3.595 seconds inside the sample. Final steady guest/host rates are
      58.9928/60.0100 and 59.0010/60.0182 per second; maximum sampled worker frames
      32.6/2.5 ms and minimum audio queues 130.2/132.1 ms. All unchanged pacing gates pass
      with no captured lifecycle or network errors. These snapshots are not whole-game
      averages. Original archive HTTP 200 matches 106724 bytes, SHA-256
      93bb0e7c9941a0682a8b429f46aac0a359c19efb3e21d89b8622c61751dee4a5. Both main and
      worker Wasm HTTP 200 bodies match the build, 17954661 bytes, SHA-256
      dda7c41e5073bdb76bf7a41ec87c1ab4876e10263b5ea522059941933d0badca.
      Initial PNG exact-hash reader failed before input at the browser's 798x598 CSS
      display size; retained as setup failure. Diagnostic-only captures validate a
      histogram reader against original native sprites, with ambiguity checks; no game,
      build or pacing-limit change. Runtime d02a603f has the same complete tree as merged
      c2798af5. This establishes bounded selection/removal/scoring and browser pacing.
      Full games, other character sets, column-collapse, persistence, audio fidelity and
      released public play remain unverified.
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
    type: sha256
    sha256: 93bb0e7c9941a0682a8b429f46aac0a359c19efb3e21d89b8622c61751dee4a5
    size_bytes: 106724
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
      Full 5388-byte ReadMe reviewed. Copyright 1994 Takeshi Takahashi; co-produced
      by Ayako and Miya, with original game-idea and Neko Neko/Youkan character credits.
      The English 1.60E executable is already present in this original package; no
      translation patch is applied. All six supplied character sets form one game intake,
      not separate variants. Independent packing matches nine files, eighteen forks and
      nine Finder records. Strict KPK end 192197, SHA-256
      f41df44d5d150df21ea4a295ebaa30618cc304a4e4954ba861ac098455a58217.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: df22d2a953499245d1b5e405d605ff035ebc34a7816ed545700e1a475d8447ab
    size_bytes: 30754
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

![Three green pieces removed for one point](https://assets.systemless.org/catalogue/media/sha256/df/df22d2a953499245d1b5e405d605ff035ebc34a7816ed545700e1a475d8447ab.png)

Click a connected group of matching characters to select it, then click again to
remove it. Larger groups earn more points. Pieces fall into gaps, and empty
columns close from the right. The original documentation and character sets
accompany the game download.
