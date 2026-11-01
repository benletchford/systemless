---
id: fatty-bears-birthday-surprise-demo
kind: game
title: Fatty Bear's Birthday Surprise Demo
summary: >-
  Help Fatty Bear prepare a midnight birthday surprise in Humongous
  Entertainment's point-and-click adventure for young children.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1993
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
      The unchanged demo archive ran natively on the 68K slice and reached the
      moonlit front-yard scene with Fatty Bear. Clicking in the scene triggered
      its animations. A matched idle control was identical until the input and
      then differed by 15,760 pixels. An exact replay matched both checkpoints,
      with zero exhausted frames. Later scenes, puzzles, audio and browser
      performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5487
runtime:
  executable_path: Fatty Bear Demo/Fatty Bear Demo
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Fatty%20Bear%20Demo.sit
    expected_sha256: c524b0e05f2f96b9fa31432d5306b525bc897702f6b6fc1a146d735029e007d0
    expected_size: 1199592
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/fatty-bears-birthday-surprise
    - https://download.classicmacdemos.com/Fatty%20Bear%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5487
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Humongous Entertainment
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under
      the policy for unchanged, freely circulated promotional demos. The
      unchanged demo appeared on period Macintosh magazine cover discs. The
      bundled documents contain no explicit redistribution grant or restriction.
      Preserve the complete unchanged archive and remove it if a rights holder
      objects.
    notes: >-
      68K demo of the children's adventure, using Humongous Entertainment's
      SCUMM-based engine.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/fatty-bears-birthday-surprise-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5487
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K front-yard scene, cropped to the game window.
references:
- https://classicmacdemos.com/fatty-bears-birthday-surprise
- https://github.com/benletchford/systemless/issues/5487
---

## A midnight surprise

![Fatty Bear's Birthday Surprise Demo](incoming/fatty-bears-birthday-surprise-demo/gameplay.png)

Fatty Bear wants to surprise Kayla on her birthday. Explore the house, find what you need and get everything ready before morning.

Click on things to explore and interact. Use the inventory along the bottom of the screen.
