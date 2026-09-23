---
id: armor-alley
kind: game
title: Armor Alley
summary: >-
  Fly a helicopter and marshal ground forces in Battle 1 of the original
  Armor Alley Macintosh demonstration.
developer: Information Access Technologies
publisher: Three-Sixty Pacific
year: 1991
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.51.0"
    architecture: 68k
    environment: >-
      Deterministic run from the unchanged Armor Alley StuffIt demo through
      its demonstration notice into Practice Battle 1, with matched input and
      no-input checkpoints
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2448
runtime:
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/armor-alley/
    download_page: https://classicmacdemos.com/armor-alley
    expected_sha256: 747dbebc3acec8ca223f76d0a1f77f8f1362e13fb90e851a0494623661ad1539
    expected_size: 318624
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/armor-alley
    - https://classicmacdemos.com/download/armor-alley/
    - https://coverdiscs.com/disc/the-macintosh-demo-games-cd
    license: Three-Sixty Pacific Armor Alley promotional demonstration distribution
    rights_holder: Armor Alley rights holders
    permission: >-
      The publisher distributed this purpose-built demonstration on The
      Macintosh Demo Games CD. The application identifies itself as a
      “Demonstration game” and limits play to Battle 1 without additional
      helicopters or funds. Classic Macintosh Game Demos documents the period
      disc and continues to offer the demo archive. Only the unchanged demo is
      staged, not the retail game.
    notes: >-
      Unchanged 318,624-byte StuffIt archive with SHA-256
      747dbebc3acec8ca223f76d0a1f77f8f1362e13fb90e851a0494623661ad1539.
      Its single packaged application is the 68K Armor Alley demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/armor-alley/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2448
    permission: >-
      Original in-game screenshot captured for this catalogue at the
      maintainer's request. Underlying Armor Alley artwork remains the
      property of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.51.0 capture from the exact demo on
      2026-09-23 after entering Practice Battle 1 and issuing keyboard input.
      The 800x600 guest framebuffer was captured as the 512x324 game-content
      viewport. PNG SHA-256
      2259d8302b7738b8fae5645cad24aea5eaa8313203957d2452a4009c47ed3f64;
      10,609 bytes.
references:
- https://classicmacdemos.com/armor-alley
- https://coverdiscs.com/disc/the-macintosh-demo-games-cd
---

## Battle 1

![Armor Alley demo helicopter and ground convoy in Battle 1](incoming/armor-alley/gameplay.png)

Armor Alley combines helicopter action with the problem of moving ground
forces across a scrolling battlefield. You fly above tanks, troops and bunkers
while spending limited funds to support a convoy toward the opposing base.

This is the original Macintosh demonstration, not the retail game. Its own
notice restricts play to Battle 1 and removes additional helicopters and
funds. The catalogue keeps its promotional StuffIt archive byte-for-byte.
Systemless reaches the practice battle, and a matched no-input run confirms
that keyboard input changes the live battle state.
