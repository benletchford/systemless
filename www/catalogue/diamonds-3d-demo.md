---
id: diamonds-3d-demo
kind: game
title: Diamonds 3D Demo
summary: Move a paddle through a 3D brick field in Varcon's arcade demo.
developer: Varcon Systems
publisher: MacSoft
year: 1995
architectures:
- 68k
- ppc
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
      Replayed the unchanged download demo, clicked NEW, and reached the first brick
      field. A mouse move shifted the paddle from the upper right to the left side. A
      matched no-input replay at the same guest tick kept the paddle in its original
      position.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3698
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged demo archive once, clicked NEW, and reached the
      active brick field. Mouse movement shifted the paddle across the field. A
      five-second active-play sample measured 60.1 host frames/s, 60.1 guest ticks/s, a 22.8 ms
      maximum frame, and a 129 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3698
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 504a95439fc60ee3f2dea59e7314935573767bd425995d53c83cc623527efbff
    size_bytes: 1492789
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/diamonds-3d
    rights_holder: Varcon Systems / MacSoft
    permission: >-
      The unchanged archive contains Diamonds 3D Download Demo 1.0d2 and its bundled
      README, which distinguishes this demo from the full CD-ROM release. No
      redistribution restriction was found in the README.
    notes: >-
      Original 1,492,789-byte StuffIt archive, SHA-256
      504a95439fc60ee3f2dea59e7314935573767bd425995d53c83cc623527efbff.
      The promoted public object matched this hash and byte count.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 6c3d89e3857591af9ac4bc15e1e06ddfff2b65298208a6c129d30f9c86c9620c
    size_bytes: 163946
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3698
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
    notes: >-
      620-by-460 Chrome capture of the active brick field after mouse input.
      The promoted public screenshot matched its recorded hash and byte count.
references:
- https://classicmacdemos.com/diamonds-3d
---

## Clear the bricks

![Diamonds 3D first level](https://assets.systemless.org/catalogue/media/sha256/6c/6c3d89e3857591af9ac4bc15e1e06ddfff2b65298208a6c129d30f9c86c9620c.png)

Click NEW to start. Move the mouse to position the paddle under the ball,
then click to release the ball. Clear the diamond bricks to finish the level.
