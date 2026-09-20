---
id: colossal-cave-adventure
kind: game
title: Colossal Cave Adventure
summary: >-
  Explore Colossal Cave, solve hazards, and discover underground treasures in
  Graham Nelson's 1995 classic Macintosh Inform release of Willie Crowther and Don
  Woods' foundational interactive fiction milestone.
developer: Willie Crowther & Don Woods
publisher: Graham Nelson
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-18
    tester: Catalogue maintainer
    systemless_version: 0.41.9
    architecture: 68k
    environment: Deterministic headless run from the Info-Mac mirror distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/272
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 3241b4bb6e18237b99d2ada629c78aace4dee8262dbcb6e84fa3619e604c7d4b
    size_bytes: 270174
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/adventure-30.hqx
    license: Freeware
    rights_holder: Willie Crowther, Don Woods & Graham Nelson
    permission: "The bundled documentation and Info-Mac submission state: \"This game is freeware. It is derived from the source code for a TADS version of this game. The TADS source is by David M. Baggett, and is copylefted under the terms of the GNU Public License.\""
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 270,174-byte file has SHA-256
      3241b4bb6e18237b99d2ada629c78aace4dee8262dbcb6e84fa3619e604c7d4b. Original game created by Willie Crowther and Don Woods in
      1977, reconstructed by David M. Baggett in 1993, ported to Inform by Graham
      Nelson in 1995, and bundled with Andrew Plotkin's MaxZip interpreter.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: e0e9d4ba20dda37fabe8a1e7cac060239c51da3698d4f30b70ab24ee84f29d85
    size_bytes: 6781
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/272
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-18, cropped exactly to the application's 480-by-428 content surface. It excludes
      the Classic Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/adventure-30.hqx
---

## The foundational interactive fiction adventure

![Colossal Cave Adventure gameplay](https://assets.systemless.org/catalogue/media/sha256/e0/e0e9d4ba20dda37fabe8a1e7cac060239c51da3698d4f30b70ab24ee84f29d85.png)

Originally designed in 1976–1977 by cave surveyor Willie Crowther and expanded by Don Woods, *Colossal Cave Adventure* established the conventions of computer interactive fiction. This 1995 classic Macintosh release presents Graham Nelson's definitive Inform rewrite (Release 3, Serial 951220), reconstructing the complete 350-point game derived from David M. Baggett's 1993 TADS edition.

Players begin standing before a small brick wellhouse at the edge of a vast forest. Upon gathering supplies—a brass lamp, keys, food, and an empty bottle—players descend into the subterranean recesses of Colossal Cave. Deep underground lies a labyrinth of twisting passages, underground streams, intricate puzzles, fierce dwarves, and legendary treasures.

## Classic Macintosh Z-Machine execution

Systemless executes *Colossal Cave Adventure* within its Motorola 68k runtime environment. Packaged inside Andrew Plotkin's MaxZip 1.4.9 interpreter, the game runs directly in modern web browsers by emulating the classic Macintosh TextEdit windowing system, Apple Event handlers, and Z-Machine bytecode dispatch without requiring external system ROMs or operating system disks.
