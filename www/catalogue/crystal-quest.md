---
id: crystal-quest
kind: game
title: Crystal Quest
summary: >-
  Sweep up crystals and dodge the aliens in Casady & Greene's original
  colorful Macintosh arcade-game sample.
developer: Patrick Buckland
publisher: Casady & Greene, Inc.
year: 1987
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "master at faa30e1975f7"
    architecture: 68k
    environment: >-
      Deterministic run from the unchanged Crystal Quest Sample v2.2.5 archive
      through its options dialog and title screen into live color gameplay
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2438
runtime:
  application_partition_size: 8388608
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/crystal-quest/
    expected_sha256: 88cf365c6985786e383acec7582ea6467b448f6f12c43cb9f46cd1fd1a552f0e
    expected_size: 252570
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/crystal-quest
    - https://classicmacdemos.com/download/crystal-quest/
    - https://static.classicmacdemos.com/demos/crystal-quest/README.txt
    license: Casady & Greene Crystal Quest promotional sample distribution
    rights_holder: Crystal Quest rights holders
    permission: >-
      Casady & Greene packaged and distributed this self-contained title as
      Crystal Quest Sample v2.2.5. Its included July 1994 read-me identifies
      the sample, explains its monitor options, and gives ordering information
      for the commercial game. Classic Macintosh Game Demos documents the
      sample on four contemporary demo discs and continues to offer this
      specific archive as the demo. Only the original sample is staged, not
      the commercial release or its modern revival.
    notes: >-
      Unchanged 252,570-byte StuffIt archive from Classic Macintosh Game Demos;
      SHA-256 88cf365c6985786e383acec7582ea6467b448f6f12c43cb9f46cd1fd1a552f0e.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/crystal-quest/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2438
    permission: >-
      Original gameplay screenshot captured for this catalogue at the
      maintainer's request. Underlying Crystal Quest artwork remains the
      property of its rights holders.
    notes: >-
      Fresh deterministic capture from the staged sample on 2026-09-23 after
      entering live color play. The 800x600 guest framebuffer was captured as
      a 527x356 game-only viewport, excluding the surrounding desktop.
      PNG SHA-256 3af891b065525c6d0d403289da716f8abc5790a5c7f4acc77e0034ba2df88d78;
      8,728 bytes.
references:
- https://classicmacdemos.com/crystal-quest
- https://static.classicmacdemos.com/demos/crystal-quest/README.txt
---

## Chase the next crystal

![Crystal Quest Sample in color gameplay](incoming/crystal-quest/gameplay.png)

In Crystal Quest, the ship glides across a single-screen arena under mouse
control. Gather every crystal while avoiding or shooting the aliens, then head
for the exit at the bottom of the playfield. Color makes the tiny enemies,
crystals and portals stand apart against the black background.

This is Casady & Greene's original Macintosh sample, not the retail game.
Holding Option during launch opens a monitor configuration dialog; the sample
can switch to color when the video device supports it. The catalogue preserves
the promotional StuffIt archive byte-for-byte and launches its 68K application.
