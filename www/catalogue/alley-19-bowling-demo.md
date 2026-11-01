---
id: alley-19-bowling-demo
kind: game
title: Alley 19 Bowling Demo
summary: >-
  Roll strikes in a lovingly rendered 1950s bowling alley with mouse-swing
  controls and a cast of retro bowlers.
developer: Mac Xpertise / StarPlay
publisher: StarPlay
year: 1996
architectures:
- ppc
default_architecture: ppc
category: Simulation
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 4e72291b deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged demo archive ran natively on the PowerPC slice. Bowl opened lane
      19 with the scoreboard and ball return. Clicking and dragging on the lane got the
      bowler up and walking to the ball return. A matched control without the mouse
      input was identical until the input and then differed by 14,370 pixels. An exact
      replay matched both checkpoints, with zero exhausted frames. Complete frames,
      scoring, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5482
runtime:
  executable_path: Alley 19™ Bowling Demo ƒ/Alley 19™ Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 4cf27b07123aff8862e56e926f23a343b1ed6f75aaf8ceb98b000a9b4acd1357
    size_bytes: 5548462
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/alley-19-bowling
    - https://download.classicmacdemos.com/Alley%2019%20Bowling%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5482
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: StarPlay
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on at least 22 period Macintosh magazine cover discs. The bundled documents
      contain no explicit redistribution grant or restriction. Preserve the complete
      unchanged archive and remove it if a rights holder objects.
    notes: Bowling demo with one lane, the Pro Shop lounge and four bowler characters.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4b9bd589d48cb1ac8bd221c3171fd14ea4a610be04a4126038e9dd14dba11e8d
    size_bytes: 572543
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5482
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native PowerPC lane 19 as the bowler walks to the ball return.
references:
- https://classicmacdemos.com/alley-19-bowling
- https://github.com/benletchford/systemless/issues/5482
---

## Hit the lanes

![Alley 19 Bowling Demo](https://assets.systemless.org/catalogue/media/sha256/4b/4b9bd589d48cb1ac8bd221c3171fd14ea4a610be04a4126038e9dd14dba11e8d.png)

Pick a bowler in the Pro Shop and head to lane 19 for a game of ten-pin bowling with a retro diner feel.

Click **Bowl** to start. Use the mouse to aim and swing, and press the space bar for options.
