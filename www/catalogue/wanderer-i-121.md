---
id: wanderer-i-121
kind: game
title: "Wanderer I: The Cult of Misery"
summary: Explore an Ultima-style world while investigating your sister's murder.
developer: Quinn Dunki
publisher: PlayMaker, Inc.
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.73.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac freeware archive opened in 68K Systemless. A new game
      passed character creation and its story screen to reach the opening world map.
      Pressing Right moved the character, while a matched idle run remained at the starting
      tile. The release browser fetched the same archive once and reproduced movement
      on the map.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3897
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 6e4e06d7f7d2b2614ff30c0b7216dccf4ec960dca668050b92a645668eaa8a26
    size_bytes: 2401309
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=4249
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/wanderer-i-121.hqx
    rights_holder: Quinn Dunki
    permission: >-
      The author's Info-Mac submission identifies this edition as freeware and
      expressly permits distribution on CD-ROM, FTP, and the web. This is the complete
      unchanged original archive.
    notes: >-
      Original 2,401,309-byte Info-Mac BinHex/StuffIt package; SHA-256
      6e4e06d7f7d2b2614ff30c0b7216dccf4ec960dca668050b92a645668eaa8a26.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a6a4781003d48373c06cfa9692a5c8ab2cafe29aeac01eee366f6bc00d4d964b
    size_bytes: 120776
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3897
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: >-
      559-by-329 game-window crop at (120,103) from an 800-by-600 Systemless
      framebuffer after the first rightward step.
references:
- https://info-mac.org/viewtopic.php?t=4249
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/adv/wanderer-i-121.hqx
---

## Begin the search

![Wanderer I opening world map after a step](https://assets.systemless.org/catalogue/media/sha256/a6/a6a4781003d48373c06cfa9692a5c8ab2cafe29aeac01eee366f6bc00d4d964b.png)

Select **Start New Game**, finish character creation with **Done**, then
click through the story screen to enter the world. Use the arrow keys to
move; the bundled documentation explains the map, inventory, and combat.
