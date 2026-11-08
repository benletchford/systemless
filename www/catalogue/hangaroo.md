---
id: hangaroo
kind: game
title: HangARoo
summary: >-
  Guess letters to reveal the phrase while a talkative kangaroo comments on your
  choices.
developer: NetCent Communications
year: 2001
architectures:
- ppc
default_architecture: ppc
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: d6633529 native candidate; browser qualification pending
    architecture: ppc
    environment: >-
      Complete unchanged author-submitted HangARoo 1.1 package, exact hangaroo
      executable. PPC/default 25 MHz/default 800x600 initial 16-bit/original partition;
      original settings unchanged. After normal intro/common title clicks and full board
      initialization, active guesses E then Q; control omits only those key edges,
      retaining run durations and common pointer motion. Populated pre-input boards match
      exactly. Active reveals E and marks its button used; Q marks its button used and the
      first red strike. Control retains hidden letters and no strike. Actual captures
      inspected. Each repeat passes four measured causal pixels and six exact prior
      captures at 1572 frontend/guest ticks, zero exhausted frames. Own references
      establish consistency, not hardware accuracy. Native binary SHA-256
      9e096a66479765d6092dd7cba5dd2a889aaec6e19473f2a40cc1d38f038327da. Native candidate includes separately
      reviewed CDM code; no dependency on that change is claimed. Full rounds, other
      phrases/categories, persistence, audio fidelity, browser pacing and released
      public play remain unverified. Launch disabled pending actual hosted-original browser
      qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5860
runtime:
  executable_path: hangaroo
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 930518f06291a72425dce0ec9a1ca9eca94e7ea279a65f816d24b21a2af4c6c1
    size_bytes: 821106
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/word/hangaroo.hqx
    - https://github.com/benletchford/systemless/issues/5860
    license: Original NCBuy Entertainment Network Application License Agreement
    rights_holder: NetCent Communications
    permission: >-
      The full embedded agreement expressly permits free redistribution provided the
      software is unchanged and all credits and copyright notices remain intact. Retain
      its prohibitions on modification, reverse engineering and bypassing
      restrictions, and all other original notices and terms. No paid sublicense, modified game or
      runtime installation.
    notes: >-
      Author-submitted version 1.1; original package metadata July 2001, copyright
      2001 in the complete 3003-byte agreement read in full. Literal legal-text SHA-256
      ea70f0361ec3511410a631239cf92abf5f49a95998287d720a71475ad50e74cd. No object-code
      disassembly/decompilation. Complete original one-app package includes its own
      Flash projector. Strict verification matches one path, both forks and Finder record;
      pack 1130755 bytes, SHA-256
      f45028c40e105ceb3ba1b259ac9946591b506d2527350c4226641781fb653e9b.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8560be032d28e25eaabc71e554921aed4749b0bb32aafc35eff25e575a0bb826
    size_bytes: 70752
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5860
    permission: Original gameplay capture of the unchanged freely distributed game.
    notes: >-
      Native exact crop (0,40)-(550,420), 550x380 without resampling, retains game
      branding and excludes desktop/menu/window chrome. Correct-letter reveal and first
      wrong-letter strike; no full-round claim.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/word/hangaroo.hqx
- https://github.com/benletchford/systemless/issues/5860
---

## Guess the phrase

![HangARoo letter guesses](https://assets.systemless.org/catalogue/media/sha256/85/8560be032d28e25eaabc71e554921aed4749b0bb32aafc35eff25e575a0bb826.png)

Type a letter to make a guess. Reveal the phrase before using all four strikes.
The complete original application and its bundled Flash projector are preserved.
