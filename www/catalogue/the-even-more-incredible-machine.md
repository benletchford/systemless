---
id: the-even-more-incredible-machine
kind: game
title: The (Even More!) Incredible Machine Demo
summary: >-
  Start a whimsical chain-reaction puzzle in Sierra's original Macintosh
  demonstration.
developer: Jeff Tunnell Productions
publisher: Sierra On-Line
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 3800c456dcb6b0539b3d170846b25ef85cf2e36c
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged demo. Skipped the animated
      introduction with Escape, reached the first tutorial puzzle, and clicked Play to run
      its physical simulation; the basketball and other objects changed positions.
      Browser launch has not yet been verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3064
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 8b02998552753041117411ec56a6f0103bd6887b68242a7403e5c21bfddf5137
    size_bytes: 658664
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/the-even-more-incredible-machine
    - https://download.classicmacdemos.com/The%20Incredible%20Demo.sit
    - >-
      https://static.classicmacdemos.com/demos/the-even-more-incredible-machine/README.txt
    rights_holder: The Incredible Machine rights holders
    permission: >-
      This is the unchanged purpose-built promotional StuffIt archive named The
      Incredible Demo, not a retail game copy. Its included ReadMe and the runnable 68K
      application are preserved intact. No express redistribution clause is present in the
      included ReadMe.
    notes: >-
      Original 658,664-byte StuffIt archive, SHA-256
      8b02998552753041117411ec56a6f0103bd6887b68242a7403e5c21bfddf5137. The archive contains the 68K The Incredible
      Demo application, its 256-color artwork, and audio resources.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b01f96573b07e90aaaeb5faeb3d9242405a1aaf0473ecac9c8fb33f916450a7e
    size_bytes: 42550
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3064
    permission: >-
      Fresh gameplay capture from the unchanged promotional demo for this catalogue
      entry. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 640-by-440 game-content crop at (80,80) of a deterministic 800-by-600
      Systemless framebuffer while the first tutorial puzzle's physical simulation was
      running. The crop excludes the Mac menu bar, window title, and desktop without
      changing game pixels. PNG SHA-256
      b01f96573b07e90aaaeb5faeb3d9242405a1aaf0473ecac9c8fb33f916450a7e, 42,550 bytes.
references:
- https://classicmacdemos.com/the-even-more-incredible-machine
- >-
  https://static.classicmacdemos.com/demos/the-even-more-incredible-machine/README.txt
---

## Put the ball in the hoop

![The Incredible Machine demo basketball tutorial puzzle in motion](https://assets.systemless.org/catalogue/media/sha256/b0/b01f96573b07e90aaaeb5faeb3d9242405a1aaf0473ecac9c8fb33f916450a7e.png)

The first tutorial puzzle asks for a basketball to pass through a hoop.
Platforms, balls, ramps, and other contraptions fill the playfield. Pressing
Play hides the setup controls and starts the simulation, letting the objects
roll and collide. This capture shows the puzzle after that change of state.

This is the original Macintosh demonstration of The (Even More!) Incredible
Machine, not the retail release. Systemless reached its interactive puzzle
with the unchanged archive. Browser launch remains disabled until manual
testing and approval.
