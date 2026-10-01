---
id: eat-my-photons-demo
kind: game
title: Eat My Photons! Demo
summary: Fly a combat mission across a surreal 3D planetary landscape.
developer: Eccentric Software
publisher: Eccentric Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, chose New, accepted the first mission,
      reached the live cockpit, and observed mouse movement change the horizon and target
      positions against a matched no-input replay at the same tick.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3687
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      In Chrome, fetched the unchanged archive once, chose New, accepted the first
      mission, reached the live cockpit, and observed the view shift after mouse
      movement. At 10 MHz, a five-second active-play sample measured 60.0 host frames/s, 60.2
      guest ticks/s, 14.3 ms maximum frame, and 145 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3687
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: ea9fd6bd23e06d8318681e0c341766e8a85cc42695fb66cc2d1d58b6ca8e9d79
    size_bytes: 1873536
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/eat-my-photons
    rights_holder: Eccentric Software
    permission: >-
      The bundled EMP Demo Read Me permits free distribution of the complete,
      unchanged demo package. The original archive includes the demo app, Data Files folder,
      and Read Me, with no retail application substituted.
    notes: >-
      Original 1,873,536-byte StuffIt archive, SHA-256
      ea9fd6bd23e06d8318681e0c341766e8a85cc42695fb66cc2d1d58b6ca8e9d79.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: dc8fd691413c2fb192fff9a9e944b3550dc9d71433895d112c46717d3d1c531d
    size_bytes: 25516
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3687
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
references:
- https://classicmacdemos.com/eat-my-photons
---

## Enter the cockpit

![Eat My Photons cockpit](https://assets.systemless.org/catalogue/media/sha256/dc/dc8fd691413c2fb192fff9a9e944b3550dc9d71433895d112c46717d3d1c531d.png)

Choose New at the demo menu, then Accept the first available mission to enter
the cockpit. Move the mouse to steer. The bundled Read Me has more controls
and setup notes.
