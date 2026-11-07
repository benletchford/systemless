---
id: jsokoban
kind: game
title: JSokoban 1.0b13
summary: >-
  Push money bags onto their destinations in Jason Townsend's Macintosh
  warehouse puzzle.
developer: Jason Townsend
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged 1.0b13 archive, exact game selector, explicit 68K and
      800x600/depth 8. Ordinary Up and five Left moves approach a money bag;
      a final Left pushes it one cell. The equal-clock control omits only the
      last Left and leaves the bag in place. Fresh active/control repeats each
      pass six measured pixels and three exact earlier Systemless captures,
      nine checks at 285 frontend/885 guest ticks, zero exhaustion. Own-run
      references establish consistency, not hardware-oracle accuracy. Native binary
      SHA-256 a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0;
      Rust/Cargo runtime trees match merged 7981331f. Independent production
      packing matches all four paths, eight forks and four Finder records against
      unar/lsar with strict end validation. The actual resource-only game has
      CODE 0/1 and no cfrg or PPC data executable. Browser input/pacing,
      completed puzzles, other levels, persistence and sound fidelity are unverified.
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
    type: incoming
    path: catalogue/incoming/jsokoban/original.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/JSokoban_1.0b13.sit
    - https://github.com/benletchford/systemless/issues/2279
    license: Original non-profit complete-package distribution grant
    rights_holder: Jason Townsend and credited contributors
    permission: >-
      The retained original Doc explicitly permits non-profit distribution with
      the Doc, Extra and Bug Report files included. Original application About
      text repeats the non-profit/full-document condition. Preserve the entire
      unchanged package, credits and notices, including Jason Best's graphics
      credit and the original Default/Extra levels and their attribution.
    notes: >-
      Original archive is 61522 bytes, SHA-256
      04452000ec19ec9a271f340e1ba1c936a3e56f3ea0ff70a0a30545060041ad91.
      All retained data/resource notices reviewed; no exclusive-media clause
      found. Original Doc is 9125 bytes. Exact application resource fork is
      87113 bytes, SHA-256 24c18890c5fedbd6f34eed7b33e881acc113283bfdd8d0024857496779a52c41.
      The original Doc identifies PPC-native support as future work. This
      independent implementation is distinct from MacSokoban and Scott Lindhurst
      Sokoban; versions and CPU slices are not counted as separate games.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/jsokoban/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2279
    permission: Original gameplay capture from the permitted unchanged package.
    notes: >-
      Native legal money-bag push. Exact 304x176 crop of the complete 19x11
      level grid at (4,42), without resampling, menu bar, window framing or
      host interface.
references:
- https://github.com/benletchford/systemless/issues/2279
---

![JSokoban after a legal money-bag push](incoming/jsokoban/gameplay.png)

Use the arrow keys or click an adjacent square to move. Push each money bag
onto a destination; bags cannot be pulled. Hold **Shift** to move as far as
possible in one direction. **Edit → Undo** takes back a push;
**File → Restart Level** starts the puzzle again.

The original package includes fifty Default levels and thirty-five Extra levels.
Native movement and a push are verified against an omitted-input control.
Browser input/pacing, completed puzzles, other levels, persistence and sound
fidelity remain unverified.
