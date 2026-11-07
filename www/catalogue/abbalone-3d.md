---
id: abbalone-3d
kind: game
title: Abbalone 3D 1.0
summary: Push opposing marbles off a three-dimensional board in this Macintosh adaptation of
  Abalone.
developer: Anomic Software
year: 1996
architectures:
- 68k
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: playable
  verified:
  - date: '2026-10-31'
    tester: Catalogue maintainer
    systemless_version: Combined 2353ccaa and PRs 5388, 5389, 5391
    architecture: ppc
    environment: The original Info-Mac archive ran natively at depth 8. After the shareware
      screen, a red marble was dragged to an adjacent empty space; four measured before-and-after
      pixel assertions passed at frontend tick 1568 with zero exhausted frames. The 68K route
      reached the initial board but its gameplay has not been established. Browser performance,
      full games, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5401
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: fc327b1bd49f148a45356a37b1d2f25aac4f5c96 native candidate
    architecture: ppc
    environment: Complete unchanged original; exact executable; original default PowerPC/25
      MHz/800x600/depth 8 and default players/skill. Ordinary Start button. Drag red marble
      (335,288) to adjacent empty (354,264); red moves and computer replies. Equal-clock control
      omits button presses and retains the original red position. Before-input captures match.
      Fresh repeats each pass eight causal pixels and two exact prior captures at frontend/guest
      612, zero exhausted frames. Actual captures inspected; own-run repeatability is not hardware
      accuracy. Binary SHA-256 ec1e070bb9deb63e84a18d0f897c28cd7197428bb3d300f84a36b98ed2c44a4f.
      Complete original Read Me and Register redistribution notice reviewed; independent extraction
      verifies four paths/eight forks/four Finder records, strict KPK end 1378943, pack SHA-256
      8aecbe2657e8bd046d898570acd196e6b6a679704d07869610d4b0ca1390ce78. Full games, wins, saves,
      audio fidelity, current 68K gameplay, browser pacing and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5401
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: f313f1f5c0d3b7429ff8dc2f8ba8dae982f55e0c private browser on fc327b1b
      runtime
    architecture: ppc
    environment: Fresh Chromium isolated worker/WebGL profiles and SharedArrayBuffer; exact
      executable; original default PPC/25 MHz/800x600/depth 8 and default players/skill. Ordinary
      blank-canvas audio gesture and Start precede each unchanged 20-second sample. Active drags
      red marble (335,288) to (354,264) and receives computer reply; no-button control follows
      the same pointer path and retains original red position. Actual captures inspected. Active/control
      steady guest rates 60.5776/59.9845, host rates 57.6465/57.0820 per second; maximum worker
      frames 35.3/29.4 ms, minimum audio queues 116.5/121.3 ms. All unchanged gates pass. Game
      ID, worker, PPC, 25 MHz, private source stamp, isolation/shared memory checked. Actual
      original archive and main/worker Wasm HTTP 200 response bodies match byte/hash pins. Wasm
      17876609 bytes, SHA-256 9999bdfda215f52c0a5537b1077858d515e7029804edba72b6203a5243e110f9.
      No lifecycle/network errors. Independent AI replies may differ; no browser determinism
      or hardware-accuracy claim. Rates are steady snapshots, not full-game averages. Current
      68K gameplay, full games, wins, saves, audio fidelity and released public play remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5401
  - date: '2026-10-31'
    tester: Catalogue maintainer
    systemless_version: Historical browser qualification; see issue 5407
    architecture: 68k
    environment: 'Earlier hosted-archive worker preview reached a populated board, but failed
      its subsequent 20-second pacing sample: 6.93 guest ticks/s, 21.51 host FPS, maximum frame
      932.3 ms, minimum audio queue 12.6 ms. No move attempted. This is retained historical
      evidence, not a current-runtime replay. The new qualification covers the original default
      PowerPC route only; current 68K gameplay remains unverified.'
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5407
runtime:
  executable_path: Abbalone 3D US 1.0/Abbalone 3D US
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: b1f4d77e09cb9aee382c928fbfc09b213e80a0ec820e44d7518d1b282c133a9a
    size_bytes: 1047025
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/abbalone-3d.hqx
    - https://github.com/benletchford/systemless/issues/5401
    license: Original noncommercial complete-package distribution notice
    rights_holder: Anomic Software
    permission: The original Read Me permits copies of the complete package on FTP and other
      Internet services, excluding MSN services. It prohibits selling the distribution or including
      it in commercial products. Preserve the complete unchanged original archive, guide, registration
      application and notices. The included Register application separately permits unmodified
      distribution.
    notes: The original is shareware with a one-month trial. The Read Me gives conflicting US$20
      and US$30 registration prices; retain the original wording and registration program without
      modification. The original application contains 68K and PowerPC routes.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3b47c66504180dd5ee726f00d387ca615c1e4a3ace0da0a0b2129585cbe00430
    size_bytes: 45588
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5401
    permission: Original gameplay capture from the permitted unchanged package, cropped to the
      game board.
    notes: Native PowerPC board after a legal player move; excludes the menu bar, window frame,
      desktop and emulator interface.
references:
- https://github.com/benletchford/systemless/issues/5401
launch_enabled: true
---

## Push the marbles

![Abbalone 3D board after a player move](https://assets.systemless.org/catalogue/media/sha256/3b/3b47c66504180dd5ee726f00d387ca615c1e4a3ace0da0a0b2129585cbe00430.png)

Move your marbles across the board and push your opponent's marbles over the
edge. Play against the Macintosh or another player. The original shareware
allows one month of use before registration is required.

Use the default PowerPC version. Current 68K gameplay remains unverified; an earlier 68K browser run failed pacing (see the linked compatibility issue).
