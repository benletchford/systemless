---
id: chiral
kind: game
title: Chiral
summary: Join coloured atoms into molecules in Ambrosia's chemistry puzzle game.
developer: Andrew Welch
publisher: Ambrosia Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: "0.83.0"
    architecture: 68k
    environment: Native headless replay of the unchanged Chiral 1.0.0 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: b4c289ac4e8ee67836281d94ce54a9a724a49802106adb49a57741c5570b81d3
    size_bytes: 1043239
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/c/
    - https://www.vintageapplemac.com/files/games/Chiral%201.0.0%20%C6%92.sit
    license: Ambrosia Software nonprofit distribution licence
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      The bundled Chiral License permits nonprofit distribution without prior written
      notice when the software is unchanged and the complete works are included.
      Distribution for profit requires written permission.
    notes: >-
      Complete ten-file original archive. The licence text states unlimited free use,
      while the game displays a 30-day shareware notice; the notice and all original
      files remain intact. No registration code or bypass is supplied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 295f518e696a85939fa444519d899bfe3b86a545c29063858412240dba687f96
    size_bytes: 486851
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4353
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Exact original archive, 68K route. Level 1 accepts placement of a purple atom
      and advances the dispenser to a red atom. The game fills the entire framebuffer;
      no desktop, menu bar or emulator framing is present.
references:
- https://www.vintageapplemac.com/software/games/c/
---

![Chiral gameplay](https://assets.systemless.org/catalogue/media/sha256/29/295f518e696a85939fa444519d899bfe3b86a545c29063858412240dba687f96.png)

Build molecules by placing coloured atoms on the board. Level 1 asks for two
molecules containing at least six atoms each.

Choose **Not Yet** at the original registration notice, then **Play Chiral**.
Click the playfield to place the atom currently shown in the dispenser.

This entry uses the complete original **1.0.0** archive. Bounded native testing
covers starting Level 1 and placing one atom; level completion, sustained play,
save persistence and browser behaviour remain unverified. Launch stays disabled
until the Systemless-hosted archive passes browser approval.
