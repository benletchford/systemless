---
id: gearheads-demo
kind: game
title: Gearheads Demo
summary: Deploy wind-up toys across a checkerboard to outscore your opponent.
developer: R/GA Interactive
publisher: Philips Media
year: 1995
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
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, opened One Player Demo, selected the
      initial toybox, reached the live board, and pressed Return to release a white toy. A
      matched no-input replay at the same tick had no white toy.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3691
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome loaded the unchanged demo archive once and reached the live one-player
      board. Return input produced active toy play and changing scores. A five-second
      active-board sample measured 60.0 host frames/s, 59.0 guest ticks/s, a 13.8 ms
      maximum frame, and a 484 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3691
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 244483a9cfff44858a6d510a2e21d14a4d71dbd644b0ae257b9dd1759fb4cb7e
    size_bytes: 896532
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/gearheads
    - https://mail.cgwmuseum.org/galleries/issues/cgw_140.pdf
    rights_holder: Philips Media / R/GA Interactive
    permission: >-
      The unchanged archive contains the original Gearheads Demo app and its support
      files. No bundled redistribution restriction was found. A contemporary
      Philips/R/GA advertisement offered a free Macintosh demo; no retail application is included.
    notes: >-
      Original 896,532-byte StuffIt archive, SHA-256
      244483a9cfff44858a6d510a2e21d14a4d71dbd644b0ae257b9dd1759fb4cb7e.
      The promoted public object matched this hash and byte count.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 17a5202aec5cf85f94074f053fa342ba2f3350e6473cc93cf1a4dcf360e62c1e
    size_bytes: 558497
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3691
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
    notes: The promoted public screenshot matched its recorded SHA-256 and byte count.
references:
- https://classicmacdemos.com/gearheads
---

## Release the toys

![Gearheads one-player board](https://assets.systemless.org/catalogue/media/sha256/17/17a5202aec5cf85f94074f053fa342ba2f3350e6473cc93cf1a4dcf360e62c1e.png)

Choose One Player Demo, select toys for the toybox, then click Play. On the
board, use Left and Right to choose a toy and Up and Down to choose a release
point. Press Return to release it. The demo's Instructions screen explains
how winding longer changes a toy's energy.
