---
id: slime-invaders
kind: game
title: Slime Invaders 2.0.1
summary: Steer a ground cannon and shoot the invading slimes.
developer: Ingemar Ragnemalm
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: "Complete unchanged original archive, explicit game executable and 68K slice, 800x600/depth 8. Game > New game starts level 1. Ordinary mouse movement steers the cannon. Holding the mouse button fires and raises the visible score to 10; the same-duration no-fire control remains at zero. Fresh asserted replays pass at 464 frontend / 1042 guest ticks, zero exhausted frames: four measured pixels and three exact prior-capture comparisons in each firing and no-fire replay. References are repeated Systemless captures, not independent hardware evidence. Native binary SHA-256 a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Production packing independently matches three original paths, six forks and three supported Finder records against unar extraction, with strict end-of-pack validation. Browser input/pacing and released/live qualification remain pending. Launch stays disabled."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5708
runtime:
  executable_path: Slime Invaders/Slime Invaders 2.0.1
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 28802adcb48be614438671072bbd2e152895eaaa300a414a727a4ca07037fd01
    size_bytes: 190113
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Slime%20Invaders.sit
    - https://github.com/benletchford/systemless/issues/5708
    license: Original noncommercial distribution grant
    rights_holder: Ingemar Ragnemalm
    permission: >-
      The original Slime Invaders docs, Legal nonsense, explicitly permit unchanged
      distribution by nonprofit organizations and noncommercial use. Commercial
      distributors must send the author a copy of the final product. Personal modifications may
      not be redistributed. Preserve the complete unchanged package and notices.
    notes: >-
      The actual game resource fork is 370756 bytes, SHA-256
      2bfc03307b8d6b6c9f1b0d9cf82830dcd841522ee34908b526349e863d233b47. CODE 0-3 and no cfrg establish 68K.
      Exact original distribution, not repacked or converted.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 62b9355e2da4044b45fcda4352307e342142e6bc1442ea1305a2ab0ea0521fd1
    size_bytes: 5924
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5708
    permission: Original gameplay capture from the permitted unchanged game package.
    notes: >-
      Native candidate 357f79d1. 510x339 gameplay/HUD crop at (145,130), without
      resampling; excludes menu bar, desktop and host/emulator UI.
references:
- https://github.com/benletchford/systemless/issues/5708
---

![Slime Invaders 2.0.1 during active gameplay](https://assets.systemless.org/catalogue/media/sha256/62/62b9355e2da4044b45fcda4352307e342142e6bc1442ea1305a2ab0ea0521fd1.png)

Choose **New game** from **Game**. Move the mouse to steer the cannon and hold the mouse button to shoot. Click to re-enter after losing a life. **Command-P** pauses and **Command-period** aborts a game.

The bounded native replay verifies steering, firing and a score increase, with a no-fire control. Later levels, codshots, high-score persistence and audio fidelity remain unverified.

Launch stays disabled pending hosted-original browser qualification and final source review.
