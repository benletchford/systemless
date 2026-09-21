---
id: battle-girl
kind: game
title: Battle-Girl
summary: >-
  Pilot a tiny armed ship through bright, crowded arenas in this fast shareware
  shooter.
developer: Ultra/United Games
publisher: Ultra/United Games
year: 1997
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-20"
    tester: Catalogue maintainer
    systemless_version: "0.42.2"
    architecture: ppc
    environment: Deterministic headless gameplay run from the original StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 07fafefccf87eaaccea627926cb8c01031a5ea36a61d6b16a98b9fe3287c31da
    size_bytes: 1378744
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/battle-girl
    - https://static.classicmacdemos.com/demos/battle-girl/README.txt
    license: Ultra/United Games demo distribution permission
    rights_holder: Ultra/United Games
    permission: >-
      The included README says the complete demonstration may be distributed freely
      by any means and to anyone, provided it is unaltered and the README remains with
      it.
    notes: >-
      Unchanged 1,378,744-byte StuffIt demo, SHA-256
      07fafefccf87eaaccea627926cb8c01031a5ea36a61d6b16a98b9fe3287c31da.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c06a1cb202bf4a99f14833a449db24ff7b00fae6da205e0fcf16cb55ac1fc46d
    size_bytes: 106450
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://classicmacdemos.com/battle-girl
    - https://static.classicmacdemos.com/demos/battle-girl/README.txt
    - https://github.com/benletchford/systemless/issues/2222
    permission: >-
      Original gameplay screenshot captured from the exact unchanged archive for this
      catalogue entry. Underlying game artwork remains the property of its rights
      holders.
    notes: >-
      Fresh deterministic Systemless capture after entering Program 1 and accepting
      movement and fire input with the Battle-Girl play probe. The framebuffer is the
      game's own 800x600 surface and contains no host UI or emulator chrome. PNG SHA-256
      c06a1cb202bf4a99f14833a449db24ff7b00fae6da205e0fcf16cb55ac1fc46d, 106,450 bytes.
references:
- https://classicmacdemos.com/battle-girl
---

## Neon arenas, one small ship

Battle-Girl is an overhead arcade shooter built around quick turns, dense
projectiles and compact arenas. This is the original PowerPC demonstration,
kept complete and unchanged under the distribution terms in its README.

Systemless launches the original PowerPC demo, enters Program 1 and responds to
mouse movement and directional fire in a deterministic gameplay run.

![Battle-Girl gameplay](https://assets.systemless.org/catalogue/media/sha256/c0/c06a1cb202bf4a99f14833a449db24ff7b00fae6da205e0fcf16cb55ac1fc46d.png)
