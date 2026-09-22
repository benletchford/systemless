---
id: color-dark-castle
kind: game
title: Color Dark Castle
summary: >-
  Enter the Great Hall in Delta Tao's original Macintosh demo of the colorful
  remake of a platform-game landmark.
developer: Delta Tao Software, Inc.
publisher: Delta Tao Software, Inc.
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic run from the unchanged promotional demo archive through its
      launcher and difficulty screen into live play in the Great Hall
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2434
runtime:
  show_menu_bar: true
  application_partition_size: 8388608
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/color-dark-castle/dark-castle-demo.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/dark-castle
    - https://download.classicmacdemos.com/Dark%20Castle%20Demo.sit
    license: Delta Tao Color Dark Castle promotional demo distribution
    rights_holder: Delta Tao Software and the Dark Castle rights holders
    permission: >-
      This purpose-built package identifies itself throughout as the Dark Castle Demo.
      Classic Macintosh Game Demos records its contemporary promotional distribution on
      four magazine and demo discs and continues to provide it specifically as the
      playable Macintosh demo. This entry preserves only those demo files; it does not
      include or claim permission for the original or remade retail games.
    notes: >-
      Unchanged 936,427-byte StuffIt archive with SHA-256
      ddbb2489d358ea5d9c89b3df44f913facea71b6f1bc3bfe9dc23ad125cb7dcc4.
      The package contains the 68K demo launcher, Color Dark Castle demo application,
      data file and preferences file.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/color-dark-castle/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2434
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request. Underlying Dark Castle artwork remains the property of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the staged demo archive
      on 2026-09-23 after entering live play in the Great Hall. The 800x600 guest
      framebuffer was cropped to the 512x342 game surface, excluding the Classic Mac
      menu bar and surrounding desktop. PNG SHA-256
      34a78ceb13102031b984bbc6e8164c38ea707bd687e51358c7becb31d53a20f3,
      112,495 bytes.
references:
- https://classicmacdemos.com/dark-castle
- https://www.deltatao.com/darkcastle/
---

## Through the castle doors

![The Great Hall in Color Dark Castle](incoming/color-dark-castle/gameplay.png)

Prince Duncan enters a castle full of guards, bats, traps and famously
unforgiving staircases. Movement is controlled from the keyboard while the
mouse aims his arm, making every thrown rock and narrow jump a deliberate act.
The Great Hall offers several routes into the castle and very little mercy once
the player chooses one.

Delta Tao's color remake keeps the staged rooms, expressive animation and
physical comedy of the 1986 Macintosh original, then redraws the castle in a
rich 256-color palette. Its demo includes the animated difficulty screen and a
playable visit to the Great Hall.

## The contemporary Macintosh demo

This is the purpose-built promotional demo of the 1994 color remake, not either
commercial release. Classic Macintosh Game Demos documents copies distributed
on four period magazine and demo discs and hosts the preserved StuffIt archive
as “Dark Castle Demo.” That archive's files use the same demo name and include
a dedicated launcher and data set.

The catalogue stages the archive without modification. Systemless extracts the
StuffIt package, launches the original 68K demo launcher, advances through the
difficulty screen and enters live play in the Great Hall using the packaged
Color Dark Castle application and data.
