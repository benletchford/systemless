---
id: blades-of-exile-demo
kind: game
title: Blades of Exile Demo
summary: Lead a party into Spiderweb's Valley of Dying Things adventure.
developer: Spiderweb Software
publisher: Spiderweb Software
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac demo opened in 68K Systemless. Created a six-character
      party, started the included Valley of Dying Things scenario, dismissed the
      scenario and help prompts, and clicked an adjacent map tile. The party moved one tile to
      the right and the map redrew around it.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3717
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once, created a party, entered the Valley
      of Dying Things scenario, and moved one tile east on a map click. At 10 MHz, a
      five-second active-map sample measured 57.6 host frames/s, 58.2 guest ticks/s, a
      23.1 ms maximum frame, and a 129 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3717
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: f27e669d7727216b9d6c675f5355c86106c0b687ea251e3c5474d58c1071069f
    size_bytes: 4477443
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/blades-of-exile-101.hqx
    - https://www.spiderwebsoftware.com/blades/macBOE.html
    rights_holder: Spiderweb Software
    permission: >-
      The bundled Spiderweb Software License explicitly permits nonprofit
      distribution of the complete, unmodified software package. This is the unchanged 1.0.1
      demonstration from the Info-Mac archive, including its license, documentation, editor,
      and Valley of Dying Things scenario. Spiderweb's current game page also states
      that Blades of Exile is free to play.
    notes: >-
      Original 4,477,443-byte BinHex/StuffIt archive, SHA-256
      f27e669d7727216b9d6c675f5355c86106c0b687ea251e3c5474d58c1071069f.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: cd8093c1406850f9604413c3cbcd0e542341c18afe110ad3aec22ed8ea6d951c
    size_bytes: 124352
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3717
    permission: >-
      Fresh Systemless gameplay capture from the unchanged demo for this catalogue
      entry. Underlying game artwork remains Spiderweb Software's property.
    notes: >-
      Exact 560-by-400 game-content crop at (119,102) from an 800-by-600 Systemless
      framebuffer after the party moved one tile in the Guest Quarters.
references:
- https://www.spiderwebsoftware.com/blades/macBOE.html
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/blades-of-exile-101.hqx
---

## The Valley of Dying Things

![The Blades of Exile party exploring the Guest Quarters](https://assets.systemless.org/catalogue/media/sha256/cd/cd8093c1406850f9604413c3cbcd0e542341c18afe110ad3aec22ed8ea6d951c.png)

Create a party, choose **Start Scenario**, and select **The Valley of Dying
Things**. Click neighboring tiles to explore Fort Talrus and begin the
investigation. The demo also includes the character and scenario editors.
