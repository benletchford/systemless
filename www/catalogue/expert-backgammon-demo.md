---
id: expert-backgammon-demo
kind: game
title: Expert Backgammon Demo
summary: >-
  Play backgammon against a strong computer opponent, with a doubling cube and
  adjustable skill levels, in this 1994 Macintosh demo.
developer: Lazarus Software
publisher: Lazarus Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 9b61c265 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice and opened
      directly to the board for game 1. Clicking on the board rolled the dice,
      and the computer moved its checkers. A matched control without the click
      was identical until the input and then differed by 5,422 pixels. An exact
      replay matched both checkpoints, with zero exhausted frames. Full games,
      the doubling cube, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5484
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 9b61c265 browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs
      of a locally served trunk build used the exact pinned original archive.
      Scripted ordinary input clicked the board to roll. Twenty-second samples
      reached 60.17, 59.02 and 58.99 guest ticks per second at 60 host fps, with
      no console errors. This approves bounded browser launch.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5484
runtime:
  executable_path: Backgammon/Expert Backgammon v.3
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Backgammon.sit
    expected_sha256: 0d40278d5f0a9f2c530f886047bfddba5e5c195ceb00670b5c34ce190d8cf651
    expected_size: 327136
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/expert-backgammon
    - https://download.classicmacdemos.com/Backgammon.sit
    - https://github.com/benletchford/systemless/issues/5484
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Lazarus Software
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under
      the policy for unchanged, freely circulated promotional demos. The
      unchanged demo appeared on period Macintosh magazine cover discs. The
      bundled documents contain no explicit redistribution grant or restriction.
      Preserve the complete unchanged archive and remove it if a rights holder
      objects.
    notes: >-
      68K demo of Expert Backgammon 3 with the full board and computer opponent.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/expert-backgammon-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5484
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K board after the dice are rolled, cropped to the game window.
references:
- https://classicmacdemos.com/expert-backgammon
- https://github.com/benletchford/systemless/issues/5484
---

## Roll the dice

![Expert Backgammon Demo](incoming/expert-backgammon-demo/gameplay.png)

Race your checkers home and bear them off before the computer does. Use the doubling cube to raise the stakes.

Click the board to roll, then drag checkers to move them. Change the difficulty from the Level menu.
