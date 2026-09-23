---
id: exile-ii-crystal-souls
kind: game
title: "Exile II: Crystal Souls"
summary: >-
  Lead a party through Spiderweb Software's original Macintosh fantasy
  role-playing demo as the Empire invades Exile.
developer: Jeff Vogel
publisher: Spiderweb Software
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.55.0"
    architecture: 68k
    environment: >-
      Release-mode browser run of the promoted unregistered demo through the
      title, welcome and party-creation dialogs into the opening Chapter I scene
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/2541
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 1b2578812a28b2477e5d891a96a8b4d7dd19cad0ab4dcbe62c8cca4ae85cdb96
    size_bytes: 2237375
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.spiderwebsoftware.com/exile2/macexile2.html
    - https://www.spiderwebsoftware.com/ftp/mac/exile2.v203.sit
    license: Spiderweb Software License included in the demo archive
    rights_holder: Spiderweb Software, Inc.
    permission: >-
      The bundled Software License permits non-profit distribution without prior
      written notice if the complete software is unmodified. This is the unchanged
      owner-hosted demo with its documentation and license intact, not the separately offered
      fully registered installer or an unlocked copy.
    notes: >-
      Original Exile II v2.0.3 StuffIt demo, 2,237,375 bytes, SHA-256
      1b2578812a28b2477e5d891a96a8b4d7dd19cad0ab4dcbe62c8cca4ae85cdb96. Spiderweb's current page says
      the game is free to play and offers free unlocking keys through its support team;
      no key is distributed here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 563e1c9a522249c266403a74f0b13aa29c2b9791978263ed121d5ddfaa88e750
    size_bytes: 116896
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2540
    permission: >-
      Fresh Systemless capture from the unregistered demo. The underlying game
      artwork remains Spiderweb Software's property.
    notes: >-
      An 800-by-600 guest-framebuffer capture of the opening Chapter I game scene,
      showing the party, map, and game controls without host desktop, browser, or
      emulator controls. PNG SHA-256
      563e1c9a522249c266403a74f0b13aa29c2b9791978263ed121d5ddfaa88e750; 116,896 bytes.
references:
- https://www.spiderwebsoftware.com/exile2/macexile2.html
- https://github.com/benletchford/systemless/issues/2540
---

## The Empire reaches Exile

![The Exile II demo at the start of Chapter I](https://assets.systemless.org/catalogue/media/sha256/56/563e1c9a522249c266403a74f0b13aa29c2b9791978263ed121d5ddfaa88e750.png)

The Empire has learned that its exiles are building a nation below the surface.
As invasion forces pour into the caves, your party searches for allies and a way
to protect Exile. This entry preserves Spiderweb Software's original Macintosh
demo, not an unlocked or modified edition.

The bundled license allows non-profit sharing of the complete, unchanged demo.
Spiderweb now describes Exile II as free to play and offers free unlocking keys
through its support team; this archive contains no key. The original license
still accompanies the download and sets the terms for unregistered use.

Systemless reaches party creation and the opening Chapter I game screen in a
deterministic run. The same path and scene were verified in the release-mode
browser build using this promoted archive.
