---
id: putt-putt-goes-to-the-moon-demo
kind: game
title: Putt-Putt Goes to the Moon Demo
summary: >-
  Help Putt-Putt the purple car find a way home from the Moon in Humongous
  Entertainment's point-and-click adventure for young children.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice and reached the lunar
      landing scene with Putt-Putt. Clicking in the scene triggered its animations. A
      matched idle control was identical until the input and then differed by 4,364
      pixels. An exact replay matched both checkpoints, with zero exhausted frames. Later
      scenes, puzzles, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5489
runtime:
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 34b6f6d13b4d87f57ae2913b91a3720d84701be4c8baaa9c9eee2462bf4d7381
    size_bytes: 1171117
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/putt-putt-goes-to-the-moon
    - https://download.classicmacdemos.com/Putt-Putt%20Moon.sit
    - https://github.com/benletchford/systemless/issues/5489
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Humongous Entertainment
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: >-
      68K demo of the children's adventure, using Humongous Entertainment's
      SCUMM-based engine.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: bbaed17b96262c8d2c68c877f384cbe1c30778c516769f776b3be242e77212a9
    size_bytes: 32357
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5489
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K lunar scene, cropped to the game window.
references:
- https://classicmacdemos.com/putt-putt-goes-to-the-moon
- https://github.com/benletchford/systemless/issues/5489
---

## Stranded on the Moon

![Putt-Putt Goes to the Moon Demo](https://assets.systemless.org/catalogue/media/sha256/bb/bbaed17b96262c8d2c68c877f384cbe1c30778c516769f776b3be242e77212a9.png)

A fireworks mishap has sent Putt-Putt to the Moon. Explore the lunar surface, meet its residents and find a way back to Cartown.

Click on things to explore and interact. Use the dashboard along the bottom of the screen.
