---
id: shade
kind: game
title: Shade (Release 3)
summary: >-
  Wait out the pre-dawn hours in your gloomy apartment in Andrew Plotkin's
  unsettling one-room text adventure.
developer: Andrew Plotkin
year: 2000
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
      PowerPC slice and printed the opening text. Typing look and pressing
      Return produced a parser response. A matched control without typing was
      identical until the input and then differed by 20429 pixels. An exact
      replay matched both checkpoints, with zero exhausted frames. The MaxZip
      status window renders as a solid black bar. Saves, longer play and browser
      performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5439
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 21bc3f3d browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs
      of a locally served trunk build used the exact pinned original archive.
      Scripted ordinary input typed LOOK and Return. Twenty-second samples
      reached 60.00, 59.99 and 60.00 guest ticks per second, with no console
      errors. This approves bounded browser launch; saves, longer play and the
      blank status bar remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5439
runtime:
  executable_path: Shade
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/shade.hqx
    expected_sha256: c6d6d24b0dff40ed1ed84d3d52db25daf2c6ef0bf7953208bde2d3178dc4de9b
    expected_size: 284671
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/shade.hqx
    - https://github.com/benletchford/systemless/issues/5439
    license: Original free distribution notice
    rights_holder: Andrew Plotkin
    permission: >-
      SHADE is copyright 2000 by Andrew Plotkin. The author's Info-Mac
      submission says it may be distributed for free, but not sold or included
      in a for-profit collection without written permission. Preserve the
      complete unchanged archive.
    notes: >-
      Stand-alone Macintosh application built with Andrew Plotkin's MaxZip Z-machine
      interpreter; the archive contains only the application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/shade/story.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5439
    permission: >-
      Fresh gameplay capture from the permitted unchanged original package. The
      story text remains its authors' property.
    notes: Native PowerPC MaxZip story window after the first command, cropped to the window.
references:
- https://github.com/benletchford/systemless/issues/5439
---

## Read, then type

![Shade (Release 3) story window](incoming/shade/story.png)

Wait out the pre-dawn hours in your gloomy apartment in Andrew Plotkin's unsettling one-room text adventure.

Type commands such as LOOK, EXAMINE DOOR or GO NORTH and press Return. Type HELP
or ABOUT for the game's own instructions.
