---
id: gridz
kind: game
title: Gridz
summary: Claim a shifting board one edge at a time in Green Dragon Creations' strategy-puzzle demo.
developer: Green Dragon Creations, Inc.
publisher: Green Dragon Creations, Inc.
year: 1997
architectures: [68k, ppc]
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.1
    architecture: 68k
    environment: Deterministic headless gameplay run from the original BinHex installer
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: a3eb2f24f75f01944dd7f7816ec7720e5250023e57bd7198c49e2d2abb1567a9
    size_bytes: 5453841
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/gridz
    - https://static.classicmacdemos.com/demos/gridz/README.txt
    license: Gridz demo distribution permission
    rights_holder: Green Dragon Creations, Inc.
    permission: >-
      The included README permits the demo installer to be freely distributed
      in its original form and forbids separate reuse of its parts.
    notes: >-
      Unchanged 5,453,841-byte BinHex installer, SHA-256
      a3eb2f24f75f01944dd7f7816ec7720e5250023e57bd7198c49e2d2abb1567a9.
references:
- https://classicmacdemos.com/gridz
---

## Draw the line

Gridz mixes territorial strategy with a board that changes as players claim
its links. This entry preserves the version 1.2 demo installer rather than an
installed or reconstructed copy.

Systemless launches the 68k application and its bundled data directly from the
original installer, reaches a new game, creates a player, and enters the live
3D board after deterministic mouse and keyboard input. PowerPC execution is
still being investigated separately, so 68k remains the default.
