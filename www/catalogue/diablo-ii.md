---
id: diablo-ii
kind: game
title: Diablo II Shareware
summary: Explore the opening of Blizzard's action role-playing sequel in its original Macintosh shareware release.
developer: Blizzard Entertainment
publisher: Blizzard Entertainment
year: 2000
architectures: [ppc]
default_architecture: ppc
category: Role-Playing
launch_enabled: false
runtime:
  screen_depth: 8
  application_partition_size: 134217728
compatibility:
  status: playable
  verified:
  - date: "2026-09-26"
    tester: Catalogue maintainer
    systemless_version: 7044bb14434783d580fca6b8ab75678d102ed14b
    architecture: ppc
    environment: >-
      Deterministic native replay of the unchanged Macintosh shareware archive.
      The EULA, Options, title menu, Barbarian selection, and character naming
      lead to the starting camp with its HUD. Clicking open ground moves the
      Barbarian and scrolls the camp. Browser launch has not yet been verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2851
  - date: "2026-09-26"
    tester: Catalogue maintainer
    systemless_version: 7044bb14434783d580fca6b8ab75678d102ed14b + local PPC fetch optimization
    architecture: ppc
    environment: >-
      Optimized local browser preview reaches the license screen. A startup
      sample measured approximately 21 guest ticks per second. Browser gameplay
      and acceptable gameplay speed remain unverified; launch stays disabled.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2878
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Diablo%20II.sit
    expected_sha256: 7a026c82ceb30ac2c3764c9cd8445822e71eeee2b51d7bca48b96a89a9254d06
    expected_size: 129335704
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/diablo-ii
    - https://www.blizzard.com/en-sg/legal/c1ae32ac-7ff9-4ac3-a03b-fc04b8697010/blizzard-legal-faq
    license: Diablo II Shareware Test License Agreement
    rights_holder: Blizzard Entertainment
    permission: >-
      The included Shareware Test License Agreement permits installation or
      distribution of additional copies on an unlimited number of computers,
      with its terms applying to every copy. The original archive is kept intact.
    notes: >-
      Unchanged 129,335,704-byte Macintosh shareware archive, SHA-256
      7a026c82ceb30ac2c3764c9cd8445822e71eeee2b51d7bca48b96a89a9254d06.
      Its application contains PowerPC PEF code and a small 68K launch stub,
      not a runnable 68K version. The included Read Me requires a G3 processor.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/diablo-ii/diablo-ii-gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2851
    permission: >-
      Fresh gameplay capture from the unchanged Macintosh shareware demo.
      The underlying game artwork remains Blizzard Entertainment's property.
    notes: >-
      Exact 640-by-480 game surface cropped at (80,60) from a Systemless
      800-by-600 framebuffer; no game pixels were altered. PNG SHA-256
      68386702029f4bad70439a64c19ad0c7a861118381e0fa01cf3a464c465e7d90;
      540,383 bytes.
references:
- https://classicmacdemos.com/diablo-ii
- https://github.com/benletchford/systemless/issues/2851
- https://github.com/benletchford/systemless/issues/2878
---

## Return to Sanctuary

This is Blizzard's original version 1.04 Macintosh shareware demo, with its
game data and license preserved in the archive. The original PowerPC game
reaches the starting camp in Systemless. Browser launch remains disabled
until browser performance passes review.

![A Barbarian in Diablo II's starting camp, running in Systemless](incoming/diablo-ii/diablo-ii-gameplay.png)
