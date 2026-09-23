---
id: snood
kind: game
title: Snood
summary: Rescue matching Snoods in David Dobson's original Macintosh shareware puzzle game.
developer: David M. Dobson
publisher: David M. Dobson
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.54.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged Snood 2.1 Macintosh shareware
      archive against the clean 0.54.0 public runtime. Dismissed the
      registration reminder and selected New Game; the full game board
      rendered. Browser interaction has not yet been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2504
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/snood/snood-2.1.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/s/
    - https://www.snoodworld.com/games/
    rights_holder: David M. Dobson / Snood rights holders
    permission: >-
      The unchanged Snood 2.1 shareware package includes its original READ ME and
      registration programs. The READ ME's license expressly allows free
      redistribution through WWW archives when those files remain included and
      the software is not modified. Registration is required for use beyond the
      shareware evaluation period; this listing supplies no registration code.
    notes: >-
      Original 602,365-byte StuffIt 5 archive, SHA-256
      d8019dd7dfa78cc52963e000e3787a277f2feb3d69f094f5256bf5a0c2592855.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/snood/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2504
    permission: >-
      Fresh deterministic game-board capture made for this catalogue entry.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Captured directly from the unchanged shareware package at a live New Game
      board. The 598-by-423 content rectangle at (112,109) excludes the Classic
      Mac menu bar and desktop without altering game pixels. PNG SHA-256
      eabafca27a96dd2efc95b5b65f05f3c8b288a48f2f842c98f2be2cb279de0cf9,
      283,944 bytes.
references:
- https://www.vintageapplemac.com/software/games/s/
- https://www.snoodworld.com/games/
---

## Rescue the Snoods

![Snood 2.1 Macintosh shareware game board](incoming/snood/gameplay.png)

Aim and launch Snoods at matching faces. Groups of three disappear, and any
Snoods they disconnect drop to safety. The Macintosh shareware edition includes
several difficulty levels and a registration reminder.

This is an unchanged original shareware package, including its read-me and
registration files, not a registered retail copy. Systemless reaches a live
game board; browser launch awaits manual approval.
