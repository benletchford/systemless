---
id: hunter-in-darkness
kind: game
title: Hunter, in Darkness
summary: >-
  Hunt the Wumpus through a pitch-dark cave maze in Andrew Plotkin's atmospheric
  text adventure.
developer: Andrew Plotkin
year: 1999
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
    systemless_version: 21bc3f3d deterministic play runner
    architecture: ppc
    environment: >-
      The original Info-Mac package opened its bundled MaxZip interpreter on the
      PowerPC slice and printed the opening text. Typing look and pressing Return produced
      a parser response. A matched control without typing was identical until the input
      and then differed by 31303 pixels. An exact replay matched both checkpoints,
      with zero exhausted frames. The MaxZip status window renders as a solid black bar.
      Saves, longer play and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5440
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 21bc3f3d browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive. Scripted
      ordinary input typed LOOK and Return. Twenty-second samples reached 60.01, 60.01 and
      60.00 guest ticks per second, with no console errors. This approves bounded
      browser launch; saves, longer play and the blank status bar remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5440
runtime:
  executable_path: Hunter, in Darkness
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 7cbf14dd7c2e0c6717671fb90f13f8b4306228952a52028899a248b7f0a857ad
    size_bytes: 294330
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/hunter-in-darkness.hqx
    - https://github.com/benletchford/systemless/issues/5440
    license: Original free distribution notice
    rights_holder: Andrew Plotkin
    permission: "\"Hunter, in Darkness\" is copyright 1999 by Andrew Plotkin. The author's Info-Mac submission says it may be copied, distributed, and played freely. Preserve the complete unchanged archive."
    notes: >-
      Stand-alone Macintosh application built with Andrew Plotkin's MaxZip Z-machine
      interpreter; the archive contains only the application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 13366a1b25baee7fa053ac36c3b3473b57a1b4623a54c8ff364cde0a06562961
    size_bytes: 3671
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5440
    permission: >-
      Fresh gameplay capture from the permitted unchanged original package. The story
      text remains its authors' property.
    notes: >-
      Native PowerPC MaxZip story window after the first command, cropped to the
      window.
references:
- https://github.com/benletchford/systemless/issues/5440
---

## Read, then type

![Hunter, in Darkness story window](https://assets.systemless.org/catalogue/media/sha256/13/13366a1b25baee7fa053ac36c3b3473b57a1b4623a54c8ff364cde0a06562961.png)

Hunt the Wumpus through a pitch-dark cave maze in Andrew Plotkin's atmospheric text adventure.

Type commands such as LOOK, EXAMINE DOOR or GO NORTH and press Return. Type HELP
or ABOUT for the game's own instructions.
