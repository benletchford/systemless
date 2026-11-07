---
id: jsokoban
kind: game
title: JSokoban 1.0b13
summary: >-
  Push money bags onto their destinations in Jason Townsend's Macintosh warehouse
  puzzle.
developer: Jason Townsend
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged 1.0b13 archive, exact game selector, explicit 68K and
      800x600/depth 8. Ordinary Up and five Left moves approach a money bag; a final Left
      pushes it one cell. The equal-clock control omits only the last Left and leaves the
      bag in place. Fresh active/control repeats each pass six measured pixels and
      three exact earlier Systemless captures, nine checks at 285 frontend/885 guest ticks,
      zero exhaustion. Own-run references establish consistency, not hardware-oracle
      accuracy. Native binary SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0; Rust/Cargo runtime trees match merged 7981331f. Independent
      production packing matches all four paths, eight forks and four Finder records against
      unar/lsar with strict end validation. The actual resource-only game has CODE 0/1
      and no cfrg or PPC data executable. Browser input/pacing, completed puzzles,
      other levels, persistence and sound fidelity are unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2279
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: eb07694a29d12f64c7ed862393c32f39eee46bc6 private browser candidate
    architecture: 68k
    environment: >-
      Optimized private HTTPS frontend on merged runtime 7981331f,
      Chromium 156.0.8078.79/Linux aarch64, isolated 68K worker/WebGL,
      25 MHz, 800x600/depth 8. Deployed COEP credentialless/COOP same-origin
      and SharedArrayBuffer verified. Actual untouched hosted archive HTTP 200
      response matches 61522 bytes and its SHA-256. Both main and worker Wasm
      response bodies match SHA-256 235771cb89f44f833822438308b18ec81316ef161a3526bb5237a6f0c4184394,
      17764881 bytes. Ordinary click on the starting player focuses the canvas;
      Up and five Left moves approach a bag, and another Left pushes it one
      square. Matched control omits only that last Left and retains the bag.
      Both samples exercise four subsequent Right/Left movements. Each unchanged
      20-second sample passes: active/control final steady guest 61.0215/60.0097,
      host 60.0045/60.0097 per second, max frame 35.1/35.0 ms, minimum audio
      queue 117.8/114.9 ms, zero captured lifecycle/network errors. These final
      snapshots are not sample-wide means. Completed puzzles, other levels,
      persistence, sound fidelity and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2279
runtime:
  executable_path: JSokoban_1.0b13/JSokoban 1.0b13
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 04452000ec19ec9a271f340e1ba1c936a3e56f3ea0ff70a0a30545060041ad91
    size_bytes: 61522
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/JSokoban_1.0b13.sit
    - https://github.com/benletchford/systemless/issues/2279
    license: Original non-profit complete-package distribution grant
    rights_holder: Jason Townsend and credited contributors
    permission: >-
      The retained original Doc explicitly permits non-profit distribution with the
      Doc, Extra and Bug Report files included. Original application About text repeats
      the non-profit/full-document condition. Preserve the entire unchanged package,
      credits and notices, including Jason Best's graphics credit and the original
      Default/Extra levels and their attribution.
    notes: >-
      Original archive is 61522 bytes, SHA-256
      04452000ec19ec9a271f340e1ba1c936a3e56f3ea0ff70a0a30545060041ad91. All retained data/resource notices reviewed; no
      exclusive-media clause found. Original Doc is 9125 bytes. Exact application resource
      fork is 87113 bytes, SHA-256
      24c18890c5fedbd6f34eed7b33e881acc113283bfdd8d0024857496779a52c41. The original Doc identifies PPC-native support as future work. This
      independent implementation is distinct from MacSokoban and Scott Lindhurst
      Sokoban; versions and CPU slices are not counted as separate games.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c9f211138c951a48e44df8048882fa1050b3d6c734aca3a1c7f45a56bfc6bd72
    size_bytes: 4437
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2279
    permission: Original gameplay capture from the permitted unchanged package.
    notes: >-
      Native legal money-bag push. Exact 304x176 crop of the complete 19x11 level
      grid at (4,42), without resampling, menu bar, window framing or host interface.
references:
- https://github.com/benletchford/systemless/issues/2279
---

![JSokoban after a legal money-bag push](https://assets.systemless.org/catalogue/media/sha256/c9/c9f211138c951a48e44df8048882fa1050b3d6c734aca3a1c7f45a56bfc6bd72.png)

Use the arrow keys or click an adjacent square to move. Push each money bag
onto a destination; bags cannot be pulled. Hold **Shift** to move as far as
possible in one direction. **Edit → Undo** takes back a push;
**File → Restart Level** starts the puzzle again.

The original package includes fifty Default levels and thirty-five Extra levels.
Native and hosted-browser movement and a push are verified against an
omitted-input control. Browser pacing passes. Completed puzzles, other levels,
persistence, sound fidelity and released public play remain unverified.
