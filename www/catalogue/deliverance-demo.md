---
id: deliverance-demo
kind: game
title: Deliverance Demo
summary: >-
  Hack through a gothic castle as a barbarian warrior in the Macintosh demo of
  21st Century Entertainment's side-scrolling action platformer.
developer: 21st Century Entertainment
publisher: 21st Century Entertainment
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice and reached the first
      castle level. A burst of arrow keys, Space and a click moved the warrior. A
      matched idle control was identical until the input and then differed by 234,932
      pixels. An exact replay matched both checkpoints, with zero exhausted frames. Combat,
      later rooms, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5486
  - date: "2026-11-02"
    tester: Catalogue maintainer
    systemless_version: d5f042bce browser preview with launch enabled
    architecture: 68k
    environment: >-
      A release-mode Chromium 156 worker/WebGL preview on macOS used the
      unchanged managed archive directly under the production origin. Three
      20-second castle-scene samples measured 60.01, 59.95 and 60.95 guest
      ticks per second, with no console errors. After Start, Space and Right,
      holding Right again scrolled the warrior into the next castle section;
      a same-duration control remained by the torch. The movement and control
      samples measured 59.28 and 60.02 guest ticks per second. Before the first
      Right input, two controls remained on a black game surface and measured
      14.24 and 15.28 ticks per second. Combat, later rooms, audio fidelity and
      the released production route remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5525
runtime:
  executable_path: Deliverance™ DEMO/Deliverance™ DEMO
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 0624f6762aa815e2443d67d121d27ddde3982de9f04fdb7be0b2a923e5621bf1
    size_bytes: 765776
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/deliverance
    - https://download.classicmacdemos.com/Deliverance%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5486
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: 21st Century Entertainment
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on at least 14 period Macintosh magazine cover discs. The bundled documents
      contain no explicit redistribution grant or restriction. Preserve the complete
      unchanged archive and remove it if a rights holder objects.
    notes: 68K demo of the action platformer (known elsewhere as Stormlord II).
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3af27ef638055e0f107ab352d3b92cc4a19e210864ea3f603a08b549b8024673
    size_bytes: 16128
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5486
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K castle level after the warrior moves, cropped to the game window.
references:
- https://classicmacdemos.com/deliverance
- https://github.com/benletchford/systemless/issues/5486
---

## Storm the castle

![Deliverance Demo](https://assets.systemless.org/catalogue/media/sha256/3a/3af27ef638055e0f107ab352d3b92cc4a19e210864ea3f603a08b549b8024673.png)

Fight your way through a castle full of monsters to rescue the captive fairies.

Click Start, then press Space and Right to enter the castle if the game screen
is black. Use the arrow keys to move and climb, and the space bar to attack.
