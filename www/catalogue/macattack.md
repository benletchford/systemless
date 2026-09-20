---
id: macattack
kind: game
title: MacAttack
summary: Battle through rendered corridors in New Reality Entertainment's three-level shareware demo.
developer: New Reality Entertainment, Inc.
publisher: New Reality Entertainment, Inc.
year: 1995
architectures: [68k]
default_architecture: 68k
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.1
    architecture: 68k
    environment: Deterministic headless gameplay run from the original MacAttack demo archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 0367d87cc9cc84df5aa28085834ad6c7a36d70325f2e07060d4abd18581f40f0
    size_bytes: 848461
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/macattack
    - https://download.classicmacdemos.com/MacAttack%20Demo.sit
    - https://static.classicmacdemos.com/demos/macattack/README.txt
    license: New Reality Entertainment shareware license
    rights_holder: New Reality Entertainment, Inc.
    permission: >-
      The included ReadMe permits complete, unmodified, non-profit
      distribution without prior notice and specifically invites uploads to
      bulletin boards. Distribution for profit or on physical media requires
      written permission.
    notes: >-
      Unchanged 848,461-byte demo archive from Classic Mac Demos,
      SHA-256 0367d87cc9cc84df5aa28085834ad6c7a36d70325f2e07060d4abd18581f40f0.
references:
- https://classicmacdemos.com/macattack
- https://www.vintageapplemac.com/software/games/m/
---

## Three corridors from a larger fight

MacAttack's shareware package contains three levels from the commercial game
and a self-running demonstration. This entry uses the complete original demo
archive preserved by Classic Mac Demos.

The current Systemless runtime reaches the live 3D playfield, responds to mouse
movement, and consumes the smart bomb after deterministic keyboard input. That
verifies gameplay rather than startup alone.
