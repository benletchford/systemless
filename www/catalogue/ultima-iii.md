---
id: ultima-iii
kind: game
title: Ultima III 1.3
summary: >-
  Form a party, explore Sosaria and confront Exodus in LairWare's classic
  Macintosh fantasy role-playing adventure.
developer: Leon McNeill / LairWare
publisher: LairWare
year: 1995
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
    environment: >-
      Deterministic headless gameplay run from the complete, unchanged Ultima
      III 1.3 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2310
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 298a95b39093c135b1a885f9999f7f0afd8d2bb10e5265ee1f40d634709d5a8b
    size_bytes: 1993800
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/u/
    - https://www.vintageapplemac.com/files/games/Ultima%20III%201.3.sit
    license: LairWare unregistered archive distribution permission
    rights_holder: Leon McNeill / LairWare; Origin Systems, Inc.
    permission: >-
      The bundled Read Me First license agreement states: “In its unregistered
      form, this software archive is freely distributable. The only requirement
      is that this software archive not be modified in any way.” This entry
      references the complete, unchanged 1,993,800-byte archive and preserves
      the included application, documentation, graphics, sounds and music.
    notes: >-
      Unchanged 1,993,800-byte StuffIt archive, pinned by SHA-256
      298a95b39093c135b1a885f9999f7f0afd8d2bb10e5265ee1f40d634709d5a8b.
      The bundled README documents the 68020-and-later requirement and describes
      the application as a fat binary native on Power Macs; the verified
      catalogue path is its 68k application slice.
references:
- https://www.vintageapplemac.com/software/games/u/
- https://www.vintageapplemac.com/files/games/Ultima%20III%201.3.sit
---

## Exodus in Sosaria

*Ultima III* is a classic party-based fantasy adventure. Create and organize a
party, equip its members, then journey across Sosaria through towns, wilderness,
sea and dungeons while gathering the strength needed to confront Exodus.

## Preserved as the complete unregistered archive

LairWare's bundled license explicitly permits distribution of the unregistered
software archive, provided it is not modified. The catalogue therefore pins the
original StuffIt archive and keeps its application, Roster, graphics, sounds,
documentation and MOD music together.

The archive is a fat Macintosh binary with a 68020-and-later 68k path and a
native PowerPC path. Systemless was tested using the verified 68k path: the
deterministic run opened the Journey Onward menu, entered the live party/map
view, and processed a right-arrow movement input. The rendered world changed
between the before/after frames, including the direction indicator changing
from `EAST` to `WEST`.
