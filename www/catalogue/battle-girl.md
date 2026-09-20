---
id: battle-girl
kind: game
title: Battle-Girl
summary: Pilot a tiny armed ship through bright, crowded arenas in this fast shareware shooter.
developer: Ultra/United Games
publisher: Ultra/United Games
year: 1997
architectures: [ppc]
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.2
    architecture: ppc
    environment: Deterministic headless gameplay run from the original StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/battle-girl/
    download_page: https://classicmacdemos.com/battle-girl
    expected_sha256: 07fafefccf87eaaccea627926cb8c01031a5ea36a61d6b16a98b9fe3287c31da
    expected_size: 1378744
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/battle-girl
    - https://static.classicmacdemos.com/demos/battle-girl/README.txt
    license: Ultra/United Games demo distribution permission
    rights_holder: Ultra/United Games
    permission: >-
      The included README says the complete demonstration may be distributed
      freely by any means and to anyone, provided it is unaltered and the
      README remains with it.
    notes: >-
      Unchanged 1,378,744-byte StuffIt demo, SHA-256
      07fafefccf87eaaccea627926cb8c01031a5ea36a61d6b16a98b9fe3287c31da.
references:
- https://classicmacdemos.com/battle-girl
---

## Neon arenas, one small ship

Battle-Girl is an overhead arcade shooter built around quick turns, dense
projectiles and compact arenas. This is the original PowerPC demonstration,
kept complete and unchanged under the distribution terms in its README.

Systemless launches the original PowerPC demo, enters Program 1 and responds to
mouse movement and directional fire in a deterministic gameplay run.
