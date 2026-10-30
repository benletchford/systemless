---
id: diablo-ii
kind: game
title: Diablo II Shareware
summary: >-
  Explore the opening of Blizzard's action role-playing sequel in its original
  Macintosh shareware release.
developer: Blizzard Entertainment
publisher: Blizzard Entertainment
year: 2000
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 731b2cf96039a9b65f6ceeaf0e639009f5195c58
    architecture: ppc
    environment: >-
      Headless native replay with an 8-bit display and 128 MiB application
      partition reached the starting camp after character creation. The camp
      remained visible through 10,300 frontend ticks; character movement was
      not exercised in this replay. This used the combined draft changes.
    status: boots
    evidence: https://github.com/benletchford/systemless/pull/5352
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 41e8021975ec50054b2ccee10b7f0e16ff792dcb
    architecture: ppc
    environment: >-
      Headless native replay of the unchanged shareware archive with an 8-bit
      display and 128 MiB application partition reached character selection and
      the loading screen. The loading screen's colors were coherent compared
      with the severely corrupted palette on the earlier baseline. Gameplay
      beyond loading was not checked in this replay.
    status: boots
    evidence: https://github.com/benletchford/systemless/pull/5355
  - date: "2026-09-26"
    tester: Catalogue maintainer
    systemless_version: 7044bb14434783d580fca6b8ab75678d102ed14b
    architecture: ppc
    environment: >-
      Deterministic native replay of the unchanged Macintosh shareware archive. The
      EULA, Options, title menu, Barbarian selection, and character naming lead to the
      starting camp with its HUD. Clicking open ground moves the Barbarian and scrolls
      the camp. This checkpoint covered native play only.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2851
  - date: "2026-09-26"
    tester: Catalogue maintainer
    systemless_version: 7044bb14434783d580fca6b8ab75678d102ed14b + local PPC fetch optimization
    architecture: ppc
    environment: >-
      Optimized local browser preview reaches the license screen. A startup sample
      measured approximately 21 guest ticks per second. Browser gameplay and acceptable
      gameplay speed were still unverified at this checkpoint.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2878
  - date: "2026-09-29"
    tester: Catalogue maintainer
    systemless_version: 79fe12926926cad1aa979de17e09f0e4b29affee
    architecture: ppc
    environment: >-
      Optimized local Chrome preview on Apple M1 with the unchanged archive.
      The browser advances through character creation and loading to the camp.
      A ground click moves the Barbarian and scrolls the camera while the HUD
      remains intact. Camp speed measured about 29 to 33 guest ticks per second
      against the original 60-tick clock, so this game runs slowly on the tested
      computer.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2878
runtime:
  application_partition_size: 134217728
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 7a026c82ceb30ac2c3764c9cd8445822e71eeee2b51d7bca48b96a89a9254d06
    size_bytes: 129335704
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/diablo-ii
    - >-
      https://www.blizzard.com/en-sg/legal/c1ae32ac-7ff9-4ac3-a03b-fc04b8697010/blizzard-legal-faq
    license: Diablo II Shareware Test License Agreement
    rights_holder: Blizzard Entertainment
    permission: >-
      The included Shareware Test License Agreement permits installation or
      distribution of additional copies on an unlimited number of computers, with its terms
      applying to every copy. The original archive is kept intact.
    notes: >-
      Unchanged 129,335,704-byte Macintosh shareware archive, SHA-256
      7a026c82ceb30ac2c3764c9cd8445822e71eeee2b51d7bca48b96a89a9254d06. Its application contains
      PowerPC PEF code and a small 68K launch stub, not a runnable 68K version. The included
      Read Me requires a G3 processor.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 68386702029f4bad70439a64c19ad0c7a861118381e0fa01cf3a464c465e7d90
    size_bytes: 540383
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2851
    permission: >-
      Fresh gameplay capture from the unchanged Macintosh shareware demo. The
      underlying game artwork remains Blizzard Entertainment's property.
    notes: >-
      Exact 640-by-480 game surface cropped at (80,60) from a Systemless 800-by-600
      framebuffer; no game pixels were altered. PNG SHA-256
      68386702029f4bad70439a64c19ad0c7a861118381e0fa01cf3a464c465e7d90; 540,383 bytes.
references:
- https://classicmacdemos.com/diablo-ii
- https://github.com/benletchford/systemless/issues/2851
- https://github.com/benletchford/systemless/issues/2878
---

## Return to Sanctuary

This is Blizzard's original version 1.04 Macintosh shareware demo, with its
game data and license preserved in the archive. The original PowerPC game
reaches the starting camp in Systemless. It also runs in the browser, though
it may play slowly on some computers.

![A Barbarian in Diablo II's starting camp, running in Systemless](https://assets.systemless.org/catalogue/media/sha256/68/68386702029f4bad70439a64c19ad0c7a861118381e0fa01cf3a464c465e7d90.png)
