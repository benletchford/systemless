---
id: castles-siege-and-conquest
kind: game
title: "Castles: Siege and Conquest"
summary: Build a medieval realm in MacPlay's original time-limited Macintosh demo.
developer: Quicksilver Software
publisher: MacPlay
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.57.0 + local site build"
    architecture: 68k
    environment: >-
      Release-mode browser launch from the immutable hosted time-limited demo
      at localhost:8080. Passed the Demo Version title, entered a player name,
      started a new game, watched the strategic-map date advance, and selected
      a map region that updated the location label to Chartres.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2652
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: 0.52.0-dev
    architecture: 68k
    environment: >-
      Deterministic headless play of the unchanged Macintosh demo at 800-by-600 in
      256 colours. Passed the title and new-game dialog, entered a player name, and
      reached the live strategic map.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2480
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 4208def1742f09b98335f69fcff215dc2ccd0beb546c95936364ccc2a69bc248
    size_bytes: 1158871
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.macintoshrepository.org/4130-castles-siege-and-conquest
    - https://info-mac.org/viewtopic.php?p=16406
    rights_holder: Interplay Productions, Inc.
    permission: >-
      This unchanged, limited Macintosh demonstration was distributed publicly to
      promote the retail game. The bundled Read Me identifies it as a demo, advertises
      ordering the full game, and documents the demo's time and feature limits. No retail
      edition is included. The archive contains no express redistribution clause.
    notes: >-
      Original 1,158,871-byte StuffIt demo archive, SHA-256
      4208def1742f09b98335f69fcff215dc2ccd0beb546c95936364ccc2a69bc248. The bundled demo ends after about a year
      and a half of game time, disables saving and loading, and includes one plot and
      one movie.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a8675f3b15cedbfb9f43df4832f2bd0b6ca4be6b95366f57e094f8305e59f60b
    size_bytes: 287522
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2480
    permission: >-
      Fresh deterministic gameplay capture made for this catalogue entry. Underlying
      game artwork remains the property of its rights holders.
    notes: >-
      Captured the 514-by-366 game-content region at (143,127) after starting a new
      game. The crop excludes the Mac menu bar and desktop without altering game pixels.
      PNG SHA-256 a8675f3b15cedbfb9f43df4832f2bd0b6ca4be6b95366f57e094f8305e59f60b,
      287,522 bytes.
references:
- https://www.macintoshrepository.org/4130-castles-siege-and-conquest
- https://info-mac.org/viewtopic.php?p=16406
---

## Rule a medieval realm

![Castles: Siege and Conquest demo strategic map](https://assets.systemless.org/catalogue/media/sha256/a8/a8675f3b15cedbfb9f43df4832f2bd0b6ca4be6b95366f57e094f8305e59f60b.png)

Choose a noble house, recruit an army, gather resources, scout neighbouring
territories, and design castles to hold your land. The strategic map ties these
tasks together as the in-game calendar advances.

## The Macintosh demo

This is MacPlay's original time-limited demonstration, not the complete
commercial game. Its bundled Read Me says the demo lasts about a year and a
half of game time, cannot save or load progress, and includes only one plot and
one movie.
