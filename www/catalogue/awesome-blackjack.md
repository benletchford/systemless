---
id: awesome-blackjack
kind: game
title: Awesome BlackJack 1.5
summary: >-
  Play a full-featured shareware blackjack table against the dealer in Bryan
  Arntson's colourful Classic Mac card game.
developer: Bryan Arntson
publisher: Bryan Arntson
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: Deterministic gameplay run from the complete unchanged Awesome BlackJack 1.5 archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2319
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: cb2e7c0d5f125664f615eafcf78c1214f9058f1b0bc4003a4ed6947bb814a19a
    size_bytes: 542035
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/a/
    - https://www.vintageapplemac.com/files/games/Awesome%20BlackJack.sit
    license: Awesome BlackJack 1.5 distribution terms
    rights_holder: Bryan Arntson
    permission: >-
      The bundled Unformatted READ ME! states: “This program may be distributed
      freely, as long as the documentation and program are distributed together
      and unaltered.” This entry therefore retains the complete unchanged
      archive, including the program, documentation and icon, together.
    notes: >-
      Unchanged 542,035-byte StuffIt archive, pinned by SHA-256
      cb2e7c0d5f125664f615eafcf78c1214f9058f1b0bc4003a4ed6947bb814a19a. The
      complete package retains Awesome BlackJack v1.5, Unformatted READ ME!,
      Awesome BJ READ ME!, and the icon. The bundled distribution grant requires
      the documentation and program to remain together and unaltered.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: e7cfa0d515888647712552a737d76f2f2d9379461b3144e6b1e1130ff96d1080
    size_bytes: 16833
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2319
    permission: Original gameplay screenshot captured for this catalogue entry.
    notes: >-
      Deterministic capture from the unchanged archive, cropped to the
      Awesome BlackJack game window and excluding the Classic Mac menu bar,
      emulator framing and host margins.
references:
- https://www.vintageapplemac.com/software/games/a/
- https://www.vintageapplemac.com/files/games/Awesome%20BlackJack.sit
---

## A full-featured blackjack table

Awesome BlackJack 1.5 is Bryan Arntson's colourful Classic Mac blackjack game.
Play the left spot against the dealer, manage the pot and bet, and use the
table's insurance, surrender, split, double, hit and stand actions as each hand
develops.

## Preserved with its distribution terms

The bundled Unformatted READ ME! permits free distribution when the
documentation and program are distributed together and unaltered. This entry
references the unchanged StuffIt archive with Awesome BlackJack v1.5, both
documentation files and the icon intact.

## In the live table

Systemless reaches the live blackjack table from the exact 68k archive. A
deterministic run dismissed the support window, opened Place Your Bet, selected
the $5 chip, and changed the displayed bet from 10 to 15, verifying gameplay
beyond launch and menu navigation.

![Awesome BlackJack gameplay](https://assets.systemless.org/catalogue/media/sha256/e7/e7cfa0d515888647712552a737d76f2f2d9379461b3144e6b1e1130ff96d1080.png)
