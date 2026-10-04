---
id: munchies
kind: game
title: Munchies 1.0.7
summary: Guide Melvin through a mouse-controlled arcade maze of food and hazards.
developer: Michael Fan
publisher: Michael Fan
year: 1996
architectures: [68k]
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-10-04"
    tester: Catalogue maintainer
    systemless_version: 0.75.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged 1.0.7 StuffIt package opens its shareware notice, title,
      and level one in headless Systemless. Matched replays at tick 2380 show
      Melvin moving when the mouse moves while the idle run leaves him near the
      starting position. The release browser build fetched the same archive once,
      opened level one with File > New Game, and responded to a mouse move while
      the score advanced. A complete level and save flow remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4011
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/munchies/Munchies_v1.0.7.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.macintoshrepository.org/4098-munchies
    rights_holder: Michael Fan and credited contributors
    permission: >-
      The bundled Read Me permits non-profit distribution without prior written
      notice when the software is unmodified and the complete works are included.
      This entry keeps the original complete shareware archive intact for free
      online play.
    notes: >-
      Original 364,612-byte StuffIt archive, SHA-256
      91396913fd0bae738750c9b8fda71e93ec905a6595afce788fac5f3c9bdc045f.
      Its SHA-1 is 220eb120a701b0dde13a3661deb10a793a97533e, matching the
      public archive listing. The archive contains every item named in the Read
      Me's package list.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/munchies/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4011
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only release browser capture of level one, excluding the Classic
      Mac menu bar and browser framing. SHA-256
      c439263583cf481c10a258cffe7f30bffd57ea5e25fa712a4a217172ee66faff.
references:
- https://www.macintoshrepository.org/4098-munchies
---

## Eat your way through level one

![Melvin in Munchies level one](incoming/munchies/gameplay.png)

Melvin follows the mouse while you gather food and dodge skulls and flying
cutlery. Dismiss the shareware notice with **Not yet**, then choose **File →
New Game** to start. Moving the mouse guides Melvin; click to fire after
collecting peapods.

This original shareware package limits some features until registration.
