---
id: aqua-blooper-piper-demo
kind: game
title: Aqua Blooper Piper Demo
summary: Join pipe pieces across a deep aquatic workroom in this 1991 puzzle demo.
developer: Bruno Steinert
publisher: Casady & Greene
year: 1991
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, used Game > New Game, and reached the live
      Level 1 pipe room with its moving conveyor.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3636
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + release browser at 10 MHz
    architecture: 68k
    environment: >-
      Fetched the exact archive once in a release-mode browser. Command-N started
      Level 1. A mouse drag took a conveyor piece into the room; after an invalid drop it
      visibly fell back. At 10 MHz a five-second active sample measured 60.0 host FPS
      and 60.2 guest ticks/sec, with an 18.3 ms maximum frame and 131 ms minimum audio
      queue. The 615-by-434 gameplay screenshot is a direct capture of the game content.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3636
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: d60afd81abcf038f14e231ded0602542fda6c86e06ccb15b13fafe0cedbc1536
    size_bytes: 278330
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/aqua-blooper-piper
    rights_holder: Bruno Steinert / Casady & Greene
    permission: >-
      The unchanged archive contains only the purpose-built Aqua Blooper Piper (DEMO)
      application, with no bundled redistribution restriction. The source catalogue
      records its distribution on four historical demo/cover discs. No retail application
      or full-game data is included.
    notes: >-
      Original 278,330-byte StuffIt archive, SHA-256
      d60afd81abcf038f14e231ded0602542fda6c86e06ccb15b13fafe0cedbc1536.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 62742e76b232b6e6d8348d8d9bf7fa75ffc8ff6835097c27067d99cfda9e73fd
    size_bytes: 38298
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3636
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners’ property.
    notes: >-
      Direct 615-by-434 capture of the active Level 1 room, excluding website
      framing, the Mac menu bar, and the window frame.
references:
- https://classicmacdemos.com/aqua-blooper-piper
- https://www.mobygames.com/game/66785/aqua-blooper-piper/
---

## Repair the pipes

![Aqua Blooper Piper Level 1 pipe room](https://assets.systemless.org/catalogue/media/sha256/62/62742e76b232b6e6d8348d8d9bf7fa75ffc8ff6835097c27067d99cfda9e73fd.png)

Press Command-N to enter the Level 1 room. Drag pieces from the conveyor toward
the open pipe end. The intro labels A and D as rotation keys; connect the pipe
across the room before time runs out.
