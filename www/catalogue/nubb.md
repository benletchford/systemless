---
id: nubb
kind: game
title: Nubb 1.22
summary: Drop pipe fittings into place and connect filled pieces to build loops.
developer: Benn Blincoe
year: 1993
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
    systemless_version: d9255200a9a892c3de8002fd975dc02802fdbae9 native polling candidate
    architecture: 68k
    environment: >-
      Complete unchanged original, exact executable, explicit 68K/default 25
      MHz/800x600/depth 8 and original default game settings. Ordinary Command-N starts a game.
      J moves the fitting clockwise to align with the filled piece; Space drops it
      beside that piece, changes the score from 0 to 7 and advances the next fitting.
      Equal-clock control retains the same J movement and omits Space, leaving the fitting
      at the edge and score zero. Before-drop captures match exactly. Fresh
      active/control repeats each pass six measured causal pixel assertions and three exact
      earlier own-run captures, nine checks at 492 frontend/1092 guest ticks, zero
      exhaustion. Both final images inspected. Own-run references establish consistency, not
      hardware-oracle accuracy. Native binary SHA-256
      09d53dffaf8730437e184ea004f80d4b8100d184263665ff43c7143c01916d17. Complete loops, level completion, high-score writes,
      sound fidelity, browser pacing and released public play remain unverified.
      Launch stays disabled pending browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5760
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 85cbcca84b810bcb2128fa07a931aa7a6fa716b1 private browser candidate
    architecture: 68k
    environment: >-
      Fresh private HTTPS frontend, runtime/build code equal to merged d9d11646
      and native d9255200, Chromium 156.0.8078.79/Linux aarch64, worker/WebGL/SAB,
      original default game settings, 25 MHz and 800x600/depth 8. Actual source stamp,
      game, architecture, clock and isolation checked. Hosted unchanged archive
      HTTP 200 matches its catalogue pin; actual main and worker Wasm HTTP 200
      responses match the independently hashed 17851910-byte build, SHA-256
      31ac3cfb90de1fea6d7a24608b6a97215bde0ce00981f0027de142fce9e9510b.
      Ordinary Command-N starts a game; visible J/L navigation aligns a fitting.
      Space places it beside the filled piece and changes score 0 to 7. Matched
      no-Space control retains score zero and only moves the perimeter fitting.
      Two further ordinary J/Space actions exercise edge traversal and rotation;
      the control retains J movements and omits Space. Fresh cases have different
      starting fitting orientations; the original filled board and score match,
      but full initial snapshot identity is not claimed. An additional within-game
      check leaves the aligned fitting untouched until about eight seconds into
      sampling, then Space places that same fitting and changes score 0 to 7.
      First-placement, control and final screenshots inspected. Separate unchanged
      20-second samples measure active/control steady guest rates 60.1035/60.8934
      ticks/sec and host rates 60.1035/58.8969 FPS, maximum worker frames 47.5/35.1
      ms and minimum audio queues 104.4/115.3 ms. The delayed-input sample also
      passes at 60.0974 guest ticks/sec and host FPS, 47.2 ms maximum worker and
      105.1 ms minimum audio queue. All existing gates pass (guest at least 50/sec,
      host at least 55 FPS, worker at most 50 ms, audio at least 40 ms), with zero
      captured lifecycle/network errors. No clock or speed preset changes; original
      automatic drop remains enabled. Steady snapshots are not full-game averages.
      Browser launch approved. Complete loops, level completion, high-score writes,
      sound fidelity, hardware-oracle accuracy and actual released public play
      remain unverified; public replay requires a human release.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5760
runtime:
  executable_path: Nubb ƒ/Nubb
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: ad153b06bb2b9977fbe9ce395db8fe1a23428a48bad6d2d16549840d77f303b1
    size_bytes: 28318
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/nubb.hqx
    - https://github.com/benletchford/systemless/issues/5760
    license: Original notice encourages noncommercial distribution
    rights_holder: Benn Blincoe
    permission: >-
      The original in-application notice encourages noncommercial distribution. Read
      Me... NOT permits passing the game along with that file unchanged. Preserve all
      original files, copyright, instructions, Top Nubbs score resource and optional $5
      shareware request. This is the first Macintosh release, version 1.22 dated June
      1993, by Benn Blincoe of laughing fish.
    notes: >-
      Independent unar/lsar extraction matches all three original paths, six forks
      and three Finder records against the unchanged production packaging code, strict
      KPK end 43190 and pack SHA-256
      d1945c555c1814141b9720d09fa31a9c8dda6ad6ecb3bf2609f259164ec444b0. The resource-only application has CODE 0/1/2 and no cfrg/PPC slice.
      Original score-list names are retained and are not author credits.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 800923d4ad1e752364f6839bc4b08f7c37bad83ed7e1e74f996652ce985cb37d
    size_bytes: 4066
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5760
    rights_holder: Benn Blincoe
    permission: Fresh gameplay capture of the unchanged distributable game.
    notes: >-
      Exact 428x294 native content crop at (48,40), excluding the Classic menu bar,
      window title and frame; no resampling. Shows a placed fitting beside the original
      filled piece, score 7 and the next fitting. Not a hardware reference.
references:
- https://github.com/benletchford/systemless/issues/5760
---

![Nubb after placing a fitting and scoring seven points](https://assets.systemless.org/catalogue/media/sha256/80/800923d4ad1e752364f6839bc4b08f7c37bad83ed7e1e74f996652ce985cb37d.png)

Start with New Game in the Game menu. Use J or Left Arrow to move clockwise,
L or Right Arrow to move counterclockwise, and Space to drop a fitting.
Drop against another fitting to place it; dropping across an empty row turns
its orientation. Connect filled pieces and close loops to clear the board.
