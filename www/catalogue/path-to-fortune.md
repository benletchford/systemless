---
id: path-to-fortune
kind: game
title: The Path to Fortune (Release 3)
summary: >-
  Set out from the city of Windhall on a fantasy quest in the first volume of
  The Windhall Chronicles.
developer: Jeff Cassidy and C.E. Forman
year: 1996
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: cc1a0a22 deterministic play runner
    architecture: ppc
    environment: >-
      The original Info-Mac package opened its bundled MaxZip interpreter on the
      PowerPC slice and printed the opening text. Typing n to decline a saved
      game, then look and pressing Return produced a parser response. A matched
      control without typing was identical until the input and then differed by
      23719 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. The MaxZip status window renders as a solid black bar.
      Saves, longer play and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5437
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: cc1a0a22 browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs
      of a locally served trunk build used the exact pinned original archive.
      Scripted ordinary input declined the saved-game prompt with N, then typed
      LOOK and Return; the story window showed the response. Twenty-second
      samples reached 59.54, 60.98 and 60.01 guest ticks per second, with no
      console errors. This approves bounded browser launch; saves, longer play
      and the blank status bar remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5437
runtime:
  executable_path: The Path to Fortune
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/path-to-fortune.hqx
    expected_sha256: 4b23687e4ccbdf35c05e57747dfcf1acb2992935a8512161197b637fbf909cfa
    expected_size: 475234
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/path-to-fortune.hqx
    - https://github.com/benletchford/systemless/issues/5437
    license: Original free distribution notice
    rights_holder: Jeff Cassidy and C.E. Forman
    permission: >-
      Copyright 1995-1996 by Jeff Cassidy and C.E. Forman. The Info-Mac
      submission says the game can be freely distributed, provided no profit is
      involved; the separate maps and hint booklet are not freely distributable
      and are not included. Preserve the complete unchanged archive.
    notes: >-
      Stand-alone Macintosh application built with Andrew Plotkin's MaxZip Z-machine
      interpreter; the archive contains only the application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/path-to-fortune/story.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5437
    permission: >-
      Fresh gameplay capture from the permitted unchanged original package. The
      story text remains its authors' property.
    notes: Native PowerPC MaxZip story window after the first command, cropped to the window.
references:
- https://github.com/benletchford/systemless/issues/5437
---

## Read, then type

![The Path to Fortune (Release 3) story window](incoming/path-to-fortune/story.png)

Set out from the city of Windhall on a fantasy quest in the first volume of The Windhall Chronicles.

Type commands such as LOOK, EXAMINE DOOR or GO NORTH and press Return. Type HELP
or ABOUT for the game's own instructions.
