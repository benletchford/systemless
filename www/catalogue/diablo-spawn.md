---
id: diablo-spawn
kind: game
title: Diablo Spawn
summary: Enter Tristram in Blizzard's original Power Macintosh Diablo demo.
developer: Blizzard Entertainment
publisher: Blizzard Entertainment
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 0dd5d22e295ab5b8bc4eae07a38091fb6e7a3cbc
    architecture: ppc
    environment: >-
      Deterministic native replay of the unchanged Macintosh demo archive. Startup
      options, the shareware menu, class choice, and character naming lead to playable
      Tristram with the game HUD. Browser launch remains unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3029
  - date: "2026-09-29"
    tester: Catalogue maintainer
    systemless_version: 82e063408e8b627b1374e0bd7db4688da0bf5096
    architecture: ppc
    environment: >-
      Local Chrome preview on Apple M1 with the unchanged archive. The browser
      advances through the startup dialog, shareware menu, class selection,
      and character naming to Tristram. A ground click moves the Warrior and
      scrolls the camera while the HUD remains visible. A steady-state sample
      measured approximately 60 guest ticks per second. The transition from
      the startup dialog includes a long black loading screen.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3329
runtime:
  application_partition_size: 33554432
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 65469596bf11cfa8c3bd367ccf69dc098e4f1f7845739ff31e826eeccd60bf6f
    size_bytes: 53455042
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/diablo
    - >-
      https://www.blizzard.com/en-sg/legal/c1ae32ac-7ff9-4ac3-a03b-fc04b8697010/blizzard-legal-faq
    rights_holder: Blizzard Entertainment
    permission: >-
      Blizzard permits noncommercial mirroring of its unaltered patches and demos
      when every original file is present and intact. This original archive is left
      unchanged.
    notes: >-
      Unchanged 53,455,042-byte Macintosh Diablo Demo 1.04 archive, SHA-256
      65469596bf11cfa8c3bd367ccf69dc098e4f1f7845739ff31e826eeccd60bf6f. The application has
      PowerPC PEF code and a 68K launch stub, not a runnable 68K edition. Its Read Me
      requires a Power Macintosh.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 125cff131af38ea34dd27752d5886167f4aa05951a82c5531a12007c3e202288
    size_bytes: 698705
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3029
    permission: >-
      Fresh gameplay capture from the unchanged Macintosh demo. The underlying game
      artwork remains Blizzard Entertainment's property.
    notes: >-
      Exact 640-by-480 game surface from Systemless native replay, with no emulator
      framing or host UI; PNG SHA-256
      125cff131af38ea34dd27752d5886167f4aa05951a82c5531a12007c3e202288; 698,705 bytes.
references:
- https://classicmacdemos.com/diablo
- https://github.com/benletchford/systemless/issues/3029
- https://github.com/benletchford/systemless/issues/3329
---

## Return to Tristram

This original Macintosh Diablo Demo 1.04 includes the PowerPC-only Diablo
Spawn application and its game data. It reaches live single-player gameplay
in Systemless and in the browser. The screen can stay black for a while during
loading before the shareware menu appears.

![A warrior in Diablo Spawn's Tristram, running in Systemless](https://assets.systemless.org/catalogue/media/sha256/12/125cff131af38ea34dd27752d5886167f4aa05951a82c5531a12007c3e202288.png)
