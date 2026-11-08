---
id: morpion
kind: game
title: Morpion
summary: Extend a solitaire dot pattern by drawing legal five-point line segments.
developer: Peter N. Lewis
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
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original and canonical executable, explicit 68K/default 25
      MHz/800x600/depth 8 and original settings. Runtime/build code equals merged master
      b5dfb40c. Command-N starts the fixed board at score 0 and 28 available moves. A
      legal drag adds a line and missing dot, changes score to 1 and available moves to
      26. Matched control moves the same pointer without buttons, retaining score 0,
      28 moves and the original board. Before-input images match exactly. Fresh repeats
      each pass eight measured causal pixel assertions and two exact prior captures,
      ten checks at 432 frontend/1032 guest ticks with zero exhaustion. Actual images
      inspected. Own-run references establish consistency, not hardware accuracy. Two
      further legal lines reach score 3 and 22 available moves. Native binary SHA-256
      35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34. Full games, scoring
      limits, save/load, sound fidelity, browser pacing and released public play remain
      unverified. Launch stays disabled pending browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5775
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 6c88a005d1467dcd7875a19d81fbdb5d2a545b45 private browser on b5dfb40c runtime
    architecture: 68k
    environment: >-
      Fresh isolated Chrome worker/WebGL active and omitted-button sessions retain
      the original settings, canonical executable and 68K/default 25 MHz/800x600/
      depth 8. Both receive the same ordinary blank-canvas audio gesture and
      Command-N setup before readiness and measurement. Each measures 20 seconds.
      Three legal active drags add lines/dots, reach score 3 and 22 available moves;
      matching pointer movement without buttons retains score 0, 28 available moves
      and the original board. Actual final images inspected. Active guest/host rates
      are 59.99/59.99 per second, minimum audio queue 109.3 ms, but maximum worker
      frame 51.6 ms fails the unchanged 50 ms gate. Control rates are 60.01/60.01,
      maximum worker frame 6.6 ms and minimum audio 132.6 ms, passing all gates.
      Game ID, worker mode, architecture, clock, isolation and shared memory are
      checked. Original archive and both main/worker Wasm HTTP 200 response bodies
      match independent byte/hash pins; no lifecycle/network errors. No lower clock,
      preset change, hidden warm-up moves or relaxed gate is used. Overall status
      remains unknown and launch disabled pending diagnosis and fresh passing
      browser qualification. No released public play is claimed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5775
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 8c871cd7 private browser on 224fd30a runtime
    architecture: 68k
    environment: >-
      Complete unchanged original, canonical executable, default 25 MHz, 800x600
      and depth 8. Native active/control each pass ten checks, including two exact
      prior captures, at 432 frontend/1032 guest ticks with zero exhaustion.
      Fresh isolated Chromium worker/WebGL sessions each measure 20 seconds.
      Command-N prepares the fixed board without gameplay moves. Three legal
      drags add lines and dots, reaching score 3 and 22 available moves; the same
      pointer movement without buttons retains score 0 and 28 available moves.
      Actual final captures inspected. Active/control steady guest rates are
      61.0153/59.9978 ticks/s and host rates 59.9984/59.9978 FPS; maximum sampled
      worker frames 46.2/7.8 ms and minimum audio queues 103.8/132.7 ms. All
      unchanged pacing gates pass. These snapshots are not whole-game averages.
      Actual game ID, architecture, 25 MHz, worker, isolation and shared memory
      verified. Original archive HTTP 200 body matches 18413 bytes and SHA-256
      36561dad5549e0d1a2a5ac41f2dc226b66606cab5b33ca4e3464575bb44cff2c.
      Both main and worker Wasm HTTP 200 bodies match 17934753 bytes and SHA-256
      64d346626ad52fdee07065b8963e30640c8f7c85ef9de38d1f1f370467043083.
      No captured lifecycle or network errors. Native binary SHA-256
      c3bf6c4e7edae44f3082deb75e2b6a640196400a352eeaa8d21b2149324ed288.
      Own-run references prove consistency, not hardware accuracy. Full games,
      scoring limits, save/load, sound fidelity and released public play remain
      unverified. Earlier failed browser evidence is retained above.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5775
runtime:
  executable_path: Morpion-100/Morpion
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 36561dad5549e0d1a2a5ac41f2dc226b66606cab5b33ca4e3464575bb44cff2c
    size_bytes: 18413
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/d/sw/games/morpion/c/morpion-100.sit
    - https://www.gryphel.com/c/sw/games/morpion/
    - https://www.gryphel.com/d/sw/games/morpion/c/morpion-10-source.sit
    - https://github.com/benletchford/systemless/issues/5775
    license: Original free noncommercial terms
    rights_holder: Peter N. Lewis
    permission: >-
      The original documentation describes Morpion as totally free and prohibits
      distribution on disks costing more than $5 without explicit permission. The original
      source release permits any noncommercial purpose. Preserve the complete unchanged
      original, all author notices, Henri Lamiraux's note and noncommercial limits.
      This is fee-free Internet distribution, not a paid disk or modified derivative.
      Documentation and source release notes were read in full.
    notes: >-
      Independent unar/lsar versus unchanged production packing verifies four
      original paths, eight forks and four Finder records, strict KPK end 32870, pack SHA-256
      f65269ea9ab83a7463680a6bcf0b69a5cc737e9579de9d32eef7642cd6f3de1e. Original inner
      paths are retained. Resource-only application has CODE 0-4 and no cfrg/PPC slice.
      The original source package remains available at the referenced URL; no external
      runtime is supplied. The archive matches the host's published MD5 and an earlier
      local copy; these are one distribution of one game, not separate catalogue
      entries.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 40d6fcbf1f35cb8c673aaf05a6169136240d4dbdd95ac93d45e97ed6cf5911f0
    size_bytes: 1319
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5775
    rights_holder: Peter N. Lewis
    permission: Fresh gameplay capture from the unchanged distributable original.
    notes: >-
      Exact 266x268 native board crop at (32,50), excluding the Classic menu bar,
      window title and frame; no resampling. Shows three legal lines and their added dots.
      The original window title carries score/available moves and is excluded with the
      window chrome. Not a hardware reference.
references:
- https://www.gryphel.com/c/sw/games/morpion/
- https://github.com/benletchford/systemless/issues/5775
---

![Morpion board after three legal lines](https://assets.systemless.org/catalogue/media/sha256/40/40d6fcbf1f35cb8c673aaf05a6169136240d4dbdd95ac93d45e97ed6cf5911f0.png)

Start a new game with Command-N. Drag a straight line through five grid points,
using four existing dots and adding one new dot, or using five existing dots.
Lines may meet at individual points but must not overlap along a segment. The
window title shows your score and the number of available moves.
