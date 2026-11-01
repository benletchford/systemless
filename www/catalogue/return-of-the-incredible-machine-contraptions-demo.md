---
id: return-of-the-incredible-machine-contraptions-demo
kind: game
title: "Return of the Incredible Machine: Contraptions Demo"
summary: >-
  Build Rube Goldberg contraptions from cats, mice, cheese, pulleys and rockets
  in Sierra's physics puzzle game.
developer: Sierra On-Line
publisher: Sierra On-Line
year: 2000
architectures:
- ppc
default_architecture: ppc
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: ppc
    environment: "The unchanged demo archive ran natively on PowerPC. Clicking past the title reached the hub, and Play Contraptions opened Tutorial #1 (Mice & Cheese). Pressing START ran the machine and the mice headed for the cheese. A matched idle control was identical until the input and then differed by 13,704 pixels. An exact replay matched every checkpoint, with zero exhausted frames. Building parts, later puzzles, and audio remain unverified. With 15 MHz pacing, three browser samples on the Tutorial #1 screen passed at about 60 host fps and 60 guest ticks/s; the default pacing reached only 54 host fps."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5499
runtime:
  runtime_pacing:
    cpu_mhz: 15
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 9f55350cbc305a698467f6c7e848fa3ec18875fa12e86472669d2f2dfd58381e
    size_bytes: 10672110
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/return-of-the-incredible-machine-contraptions
    - https://download.classicmacdemos.com/Contraptions%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5499
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Sierra On-Line
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: >-
      PowerPC demo with ten single-player contraptions, three two-player puzzles and
      twenty parts.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 64c93c1f164e992d0d538c00129bec64aa5762b0f1e8ab76d32b019fdb2dde5c
    size_bytes: 403610
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5499
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native PowerPC Tutorial
references:
- https://classicmacdemos.com/return-of-the-incredible-machine-contraptions
- https://github.com/benletchford/systemless/issues/5499
---

## Mice and cheese

![Return of the Incredible Machine: Contraptions Demo](https://assets.systemless.org/catalogue/media/sha256/64/64c93c1f164e992d0d538c00129bec64aa5762b0f1e8ab76d32b019fdb2dde5c.png)

Each puzzle sets a goal and gives you a handful of parts. Place them, press START and watch the chain reaction play out.

Drag parts from the bin into the machine, then click START. Click STOP to rearrange.
