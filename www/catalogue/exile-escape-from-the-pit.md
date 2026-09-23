---
id: exile-escape-from-the-pit
kind: game
title: "Exile: Escape from the Pit"
summary: >-
  Lead a party of outcasts through Spiderweb Software's original Macintosh
  fantasy role-playing demo.
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
      title screen, new-party and prefab-party dialogs, introduction, and
      responsive Fort Exile scene
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/2534
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 79ab3bd6cacc8f95b3b80781232ce7cfc1450096623ef6f620311b3459d8c899
    size_bytes: 1677897
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.spiderwebsoftware.com/exile/macexile.html
    - https://www.spiderwebsoftware.com/ftp/mac/exile.v201.sit
    license: Spiderweb Software License included in the demo archive
    rights_holder: Spiderweb Software, Inc.
    permission: >-
      The bundled Software License permits non-profit distribution without prior
      written notice if the complete software is unmodified. This is the unchanged
      owner-hosted demo, with its documentation and license intact, not the separately offered
      fully registered installer or an unlocked copy.
    notes: >-
      Original Exile v2.0.1 StuffIt demo, 1,677,897 bytes, SHA-256
      79ab3bd6cacc8f95b3b80781232ce7cfc1450096623ef6f620311b3459d8c899. Spiderweb's current Exile page
      says the game is free to play and offers free unlocking keys by contacting its
      support team; no key is distributed here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 94fdc0e74c5cfe519f91b1ac3fb82df257e285210aaea708ce81e16565c318a9
    size_bytes: 53322
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2529
    permission: >-
      Fresh Systemless capture from the unregistered demo. The underlying game
      artwork remains Spiderweb Software's property.
    notes: "A 535-by-433 game-window content crop showing the starting Fort Exile map, party and controls. Captured from this exact archive with the generic File Manager correction tracked in issue #2530; no Classic Mac menu bar, desktop, browser, or emulator controls are included. PNG SHA-256 94fdc0e74c5cfe519f91b1ac3fb82df257e285210aaea708ce81e16565c318a9, 53,322 bytes."
references:
- https://www.spiderwebsoftware.com/exile/macexile.html
- https://github.com/benletchford/systemless/issues/2529
- https://github.com/benletchford/systemless/issues/2530
---

## The world beneath the world

![Exile demo at Fort Exile](https://assets.systemless.org/catalogue/media/sha256/94/94fdc0e74c5cfe519f91b1ac3fb82df257e285210aaea708ce81e16565c318a9.png)

Banished from the Empire, your party arrives in the underground world of Exile.
Build a party, explore its towns and caves, and decide how the outcasts will
survive. This entry preserves Spiderweb Software's original Macintosh demo, not
an unlocked or modified edition.

The bundled license allows non-profit sharing of the complete, unchanged demo.
Spiderweb now describes Exile as free to play and offers free unlocking keys
through its support team; this archive contains no key. The original license
still accompanies the download and sets the terms for unregistered use.

With the generic File Manager correction, Systemless reaches the starting Fort
Exile scene. The same scene and its new-party path were verified in the
release-mode browser build using this promoted archive.
