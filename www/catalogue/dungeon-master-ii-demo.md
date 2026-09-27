---
id: dungeon-master-ii-demo
kind: game
title: "Dungeon Master II: The Legend of Skullkeep Demo"
summary: Enter Skullkeep in MacPlay's original first-person Macintosh demonstration.
developer: Software Heaven, Inc.
publisher: MacPlay / Interplay Productions
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 69a62f0608af370f0f26a1295a8719e3b09b3f2f
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged demo. Dismissed the promotional
      screen, selected New, reached the first-person dungeon with the champion and
      movement controls, then clicked forward and observed the corridor view change.
      The promoted archive and screenshot were fetched back and matched their
      recorded SHA-256 hashes. Native-Mac comparison and browser launch have
      not yet been verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3079
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 0c14eae6013bbc2867150fa5c3315ff0e7baef674a4d9892d8159ce742caa7f7
    size_bytes: 6123827
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/dungeon-master-ii-the-legend-of-skullkeep
    - https://download.classicmacdemos.com/Dungeon%20Master%202.sit
    - >-
      https://static.classicmacdemos.com/demos/dungeon-master-ii-the-legend-of-skullkeep/README.txt
    rights_holder: Dungeon Master II rights holders
    permission: >-
      MacPlay packaged this as a standalone promotional demo with its own Read Me and
      order form, distinct from the retail release. The included Read Me calls it a
      demo, provides minimum requirements and gameplay guidance, and contains no express
      redistribution prohibition. This entry preserves the exact unchanged demo
      archive, not a retail copy or a modified version.
    notes: >-
      Original 6,123,827-byte StuffIt archive, SHA-256
      0c14eae6013bbc2867150fa5c3315ff0e7baef674a4d9892d8159ce742caa7f7. Includes the 68K demo application, graphics
      and dungeon data, promotional movies, Read Me, and order form. The Read Me
      specifies a 25 MHz 68030 minimum.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8af08121d80c6d30d6c32ed0f109a20a7edd6b2fe586865a4e81f25027d1459a
    size_bytes: 229790
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3079
    permission: >-
      Fresh gameplay capture made for this catalogue entry from the unchanged
      promotional demo. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 512-by-384 game-content crop at (144,108) of a deterministic 800-by-600
      Systemless framebuffer after a forward movement click. The surrounding desktop was
      excluded without changing game pixels. PNG SHA-256
      8af08121d80c6d30d6c32ed0f109a20a7edd6b2fe586865a4e81f25027d1459a, 229,790 bytes.
references:
- https://classicmacdemos.com/dungeon-master-ii-the-legend-of-skullkeep
- >-
  https://static.classicmacdemos.com/demos/dungeon-master-ii-the-legend-of-skullkeep/README.txt
---

## Into the corridor

![Dungeon Master II demo first-person dungeon](https://assets.systemless.org/catalogue/media/sha256/8a/8af08121d80c6d30d6c32ed0f109a20a7edd6b2fe586865a4e81f25027d1459a.png)

The demonstration starts with a lone champion in a stone chamber. A first-person
view, inventory slots, and direction controls frame the opening choices. Clicking
forward moves the view into the corridor; the included Read Me explains how to
inspect nearby chambers, recruit companions, handle items, and cast spells.

This is MacPlay's original 68K Macintosh demonstration, not the retail game.
Systemless reaches the dungeon and accepts movement input from the unchanged
archive. Browser launch remains disabled until a manual check is complete.
