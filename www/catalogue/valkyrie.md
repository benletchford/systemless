---
id: valkyrie
kind: game
title: Valkyrie
summary: Take a helicopter up for a three-minute unrestricted flight in GameTek's compact simulator demo.
developer: GameTek, Inc.
publisher: GameTek, Inc.
year: 1993
architectures: [68k]
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.1
    architecture: 68k
    environment: Deterministic headless gameplay run from the original StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2241
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: bf30b8d9de82f543f2f68ca0dcdb45d3dc915eccaa97c856473bcb499d62f030
    size_bytes: 383018
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/valkyrie
    - https://static.classicmacdemos.com/demos/valkyrie/README.txt
    license: Valkyrie demo sharing permission
    rights_holder: GameTek, Inc.
    permission: The included README expressly invites players to give the demo to friends.
    notes: >-
      Unchanged 383,018-byte demo archive, SHA-256
      bf30b8d9de82f543f2f68ca0dcdb45d3dc915eccaa97c856473bcb499d62f030.
references:
- https://classicmacdemos.com/valkyrie
---

## Three minutes in the air

Valkyrie's demonstration alternates between a recorded flight and three
minutes of unrestricted helicopter control. This is the complete original 68k
demo archive. Systemless launches the demo, completes its replay sequence,
switches into PLAY mode, and responds to flight and weapon controls during a
deterministic run of the original archive.
