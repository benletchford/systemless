---
id: 3d-ultra-pinball
kind: game
title: 3-D Ultra Pinball Demo
summary: >-
  Launch a ball onto Sierra's Outpost-themed pinball table in the original
  Macintosh demo.
developer: Dynamix
publisher: Sierra On-Line
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 74826abcdfdca890db658443d9d601da5f2fbc41
    architecture: 68k
    environment: >-
      Deterministic replay of the unchanged demo reached the live Outpost table,
      launched a ball by holding and releasing Down Arrow, and actuated both flippers with
      Control and Shift. The same script reached those gameplay checkpoints in
      BasiliskII. Browser launch remains unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3041
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: becae8fd53da4d7287505a0507440214ddc3cb26e0dbb23c2d3a88c7f03a8f2d
    size_bytes: 3753661
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/3d-ultra-pinball
    - https://download.classicmacdemos.com/3D%20Ultra%20Pinball%20Demo.sit
    - https://static.classicmacdemos.com/demos/3d-ultra-pinball/README.txt
    rights_holder: 3-D Ultra Pinball rights holders
    permission: >-
      This is the unchanged purpose-built promotional Macintosh demo, not the retail
      game. Its included Read Me identifies it as a demo, documents the plunger and
      flipper controls, and distinguishes it from the configurable full version. Classic
      Macintosh Game Demos records distribution on contemporary cover discs. No express
      redistribution clause is present.
    notes: >-
      Original 3,753,661-byte StuffIt archive, SHA-256
      becae8fd53da4d7287505a0507440214ddc3cb26e0dbb23c2d3a88c7f03a8f2d. The tested 3D Pinball Demo application has a
      runnable 68K CODE resource.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a4bd03f5578a8ce7910e467fe8c7b141101a6ddfc7e29c89b8e60622354b7f16
    size_bytes: 472752
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3041
    permission: >-
      Fresh gameplay capture from the unchanged promotional demo for this catalogue
      entry. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 640-by-480 game-content crop at (80,80) of a deterministic 800-by-600
      Systemless framebuffer while both flippers were held. The crop excludes the desktop
      and game-window frame without changing game pixels. PNG SHA-256
      a4bd03f5578a8ce7910e467fe8c7b141101a6ddfc7e29c89b8e60622354b7f16, 472,752 bytes.
references:
- https://classicmacdemos.com/3d-ultra-pinball
- https://static.classicmacdemos.com/demos/3d-ultra-pinball/README.txt
---

## The Outpost table

![3-D Ultra Pinball demo Outpost table with both flippers raised](https://assets.systemless.org/catalogue/media/sha256/a4/a4bd03f5578a8ce7910e467fe8c7b141101a6ddfc7e29c89b8e60622354b7f16.png)

The demonstration opens directly onto an Outpost-inspired pinball table.
Hold Down Arrow to charge the plunger, release it to send a ball into play,
then use Control and Shift for the left and right flippers. The table's ramps,
targets, and animated score display remain active during play.

This is the original Macintosh promotional demo, not the retail release.
The same gameplay sequence reached the table and exercised its controls in
Systemless and BasiliskII. Browser launch remains disabled until manual
testing and approval.
