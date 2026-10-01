---
id: syndicate-demo
kind: game
title: Syndicate Demo
summary: Command four cybernetic agents in Bullfrog's isometric tactical demo.
developer: Bullfrog Productions
publisher: Electronic Arts
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged demo, accepted its 16-colour display setting, and
      advanced through the world map, mission brief, and team selection to an active mission.
      A matched replay at the same guest tick showed a map click move the selected
      agent; the no-input replay left that agent at the starting position.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3704
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once, accepted the display setting, and
      advanced through the world map, mission brief, and team selection to the live
      mission. The mission continued after a map click. A five-second active-play sample
      measured 60.1 host frames/s, 60.3 guest ticks/s, a 16.4 ms maximum frame, and a 125
      ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3704
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: feee55295a8caa41a7d7de6e3eb7bcfd82eb3b56c7edde9133e47f2c3606cbf0
    size_bytes: 726824
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/syndicate
    rights_holder: Bullfrog Productions / Electronic Arts
    permission: >-
      The unchanged StuffIt package contains the original MacSyndicate Demo
      application and its DATA directory. The title screen identifies it as a demo version. No
      bundled redistribution restriction was found.
    notes: >-
      Original 726,824-byte StuffIt archive, SHA-256
      feee55295a8caa41a7d7de6e3eb7bcfd82eb3b56c7edde9133e47f2c3606cbf0.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: badf5fdcd3bfabf7c350653bf911626821db5c5b5da35966d33ff5045a7d8578
    size_bytes: 104181
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3704
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
    notes: 640-by-400 content-only capture of the live mission after moving an agent.
references:
- https://classicmacdemos.com/syndicate
---

## Direct the agents

![Syndicate demo mission](https://assets.systemless.org/catalogue/media/sha256/ba/badf5fdcd3bfabf7c350653bf911626821db5c5b5da35966d33ff5045a7d8578.png)

Accept the 16-colour display setting, click past the title screen, choose the
mission brief, then accept the team. Click the mission map to move the selected
agent.
