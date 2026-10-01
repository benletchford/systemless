---
id: zoa-demo
kind: game
title: ZOA Demo
summary: Defend a space station from alien craft in a 3D flight shooter.
developer: Julian James
publisher: Casady & Greene
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged demo, opened Game > New Game, and reached the
      spaceflight cockpit. A matched replay at the same guest tick showed
      mouse movement shift the sight and view compared with no input.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3701
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once, opened Game > New Game,
      and reached active spaceflight. Mouse movement changed the sight and
      view. A five-second active-play sample measured 59.5 host frames/s,
      60.1 guest ticks/s, a 19.9 ms maximum frame, and a 112 ms minimum
      audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3701
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/zoa-demo/archive.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/zoa-the-zone-of-avoidance
    rights_holder: Casady & Greene / Julian James
    permission: >-
      The unchanged StuffIt archive contains the original ZOA Demo application
      with its sound and texture data. The bundled Read Me identifies it as a
      limited demonstration and distinguishes the full version. No
      redistribution restriction was found in the Read Me.
    notes: >-
      Original 687,693-byte StuffIt archive, SHA-256
      a963ddf39bb834cf4da1ff716585e21c18fa2aa2dfa1336dad2b8859c22a1822.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/zoa-demo/screenshot.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3701
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue
      entry. The underlying game artwork remains its owners' property.
    notes: 512-by-348 direct Chrome capture of the cockpit after mouse input.
references:
- https://classicmacdemos.com/zoa-the-zone-of-avoidance
---

## Defend the station

![ZOA spaceflight cockpit](incoming/zoa-demo/screenshot.png)

Open Game and choose New Game. Move the mouse to steer the view toward
targets. The Controls menu offers mouse and keyboard control settings.
