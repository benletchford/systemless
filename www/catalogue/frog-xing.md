---
id: frog-xing
kind: game
title: Frog Xing 1.6
summary: >-
  Hop a frog across a busy road and a river of logs and turtles in Quarter Note
  Software's colourful road-crossing game.
developer: Quarter Note Software
year: 2000
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 43636404 deterministic play runner
    architecture: ppc
    environment: >-
      The original Info-Mac archive ran natively at depth 8. After the shareware
      notice was dismissed with Not Yet, which needed the InvertRoundRect repair in
      78cc2755, File > New Game started level 1. Two Up presses moved the frog into the first
      road lane. A matched control without the keys was identical until the input and
      then differed by 784 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. Full levels, scores, audio and browser performance remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5419
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 43636404 deterministic play runner
    architecture: 68k
    environment: >-
      The same input sequence on the 68K slice moved the frog into the first lane.
      The matched no-key control differed by 485 pixels after the input, and an exact
      replay matched both checkpoints.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5419
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 78ec4f28 browser build with launch flag and 10 MHz pacing
    architecture: 68k
    environment: >-
      The PowerPC slice paced below the browser minimum (43.7 guest ticks per second
      at 25 MHz; 43.5 to 46.7 host fps at 10 MHz). The 68K slice is the default and
      runs with runtime pacing at 10 MHz. Three release-mode headless Chromium 156
      (aarch64 Linux) worker/WebGL runs of a locally served trunk build used the
      actual Systemless-hosted archive, whose bytes matched the pinned size and
      SHA-256. Scripted ordinary input dismissed the shareware notice, started a
      game with Command-N and pressed Up; live play was visible. Twenty-second
      samples reached 60.00, 60.03 and 60.94 guest ticks per second (host 60.00,
      61.06 and 60.94 fps), with no console errors. This approves bounded browser
      launch; full levels and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5419
runtime:
  executable_path: Frog Xing 1.6 ƒ/Frog Xing 1.6
  screen_depth: 8
  show_menu_bar: true
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 34c96a3df2e1dbec5de4607ffb8c7901680478357c8c22d3c03d3239fa55a99a
    size_bytes: 1234598
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/frog-xing-16.hqx
    - https://github.com/benletchford/systemless/issues/5419
    license: Original shareware distribution notice
    rights_holder: Quarter Note Software
    permission: >-
      The game's About text encourages distribution, provided no changes are made and
      all supporting documentation is included. The Read Me separately asks CD
      distributors to obtain permission. Preserve the complete unchanged archive, Read Me,
      troubleshooting notes and registration program.
    notes: >-
      US$15 shareware with a 30-day trial notice. The fat application contains 68K
      and PowerPC code.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: df43015d518f68f1cd16e5fa294bc1b46591687e1f43a1822fa8b917963779ae
    size_bytes: 3743
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5419
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: >-
      Native PowerPC play field after the frog hops into the first lane, cropped to
      the game window contents.
references:
- https://github.com/benletchford/systemless/issues/5419
---

## Cross the road

![Frog Xing frog hopping into traffic](https://assets.systemless.org/catalogue/media/sha256/df/df43015d518f68f1cd16e5fa294bc1b46591687e1f43a1822fa8b917963779ae.png)

Guide your frog across lanes of traffic, then ride logs and turtles over the river
to reach a home bay. Later levels speed things up.

Click **Not Yet** on the shareware notice, then choose **File > New Game**. Use
the arrow keys to hop.
