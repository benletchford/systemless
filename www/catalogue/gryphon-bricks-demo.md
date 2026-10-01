---
id: gryphon-bricks-demo
kind: game
title: Gryphon Bricks Demo
summary: Build on a virtual baseplate with Gryphon's limited brick set.
developer: Gryphon Software Corporation
publisher: Gryphon Software Corporation
year: 1996
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
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged demo to its construction board. A matched no-input
      replay left the baseplate empty; selecting a brick from the palette and clicking the
      baseplate placed a red brick in the input replay.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3708
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once and opened the construction board.
      Selecting a brick and clicking the baseplate placed it. A five-second sample
      measured 60.0 host frames/s, 60.2 guest ticks/s, a 21.9 ms maximum frame, and a 128 ms
      minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3708
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: ea98d50fc10e25feb10a995de2c45dfaea3efe8e01a633889329754b31552f21
    size_bytes: 168278
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/gryphon-bricks
    rights_holder: Gryphon Software Corporation
    permission: >-
      The unchanged StuffIt archive contains the original Gryphon Bricks 1.0 Demo,
      its Read Me, and sample documents. The Read Me identifies a limited demonstration
      with four brick types and disabled save and print features. No redistribution
      restriction was found in the Read Me.
    notes: >-
      Original 168,278-byte StuffIt archive, SHA-256
      ea98d50fc10e25feb10a995de2c45dfaea3efe8e01a633889329754b31552f21.
      The promoted public object matched this hash and byte count.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ac2e42cd7de856219a935469f65db6df263e34c6d273cb367379ad6a934fbdc4
    size_bytes: 8583
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3708
    permission: >-
      Fresh construction capture made from the original demo for this catalogue
      entry. The underlying artwork remains its owner's property.
    notes: >-
      523-by-410 content-only capture of the baseplate after placing a brick.
      The promoted public screenshot matched its recorded hash and byte count.
references:
- https://classicmacdemos.com/gryphon-bricks
---

## Build on the baseplate

![A brick placed on the Gryphon Bricks baseplate](https://assets.systemless.org/catalogue/media/sha256/ac/ac2e42cd7de856219a935469f65db6df263e34c6d273cb367379ad6a934fbdc4.png)

Choose a brick from the palette on the left, then click the baseplate to place
it. The demo includes four brick types and sample constructions.
