---
id: so-far
kind: game
title: So Far (Release 6)
summary: >-
  Step from a stuffy theatre into a series of strange, shifting worlds in Andrew
  Plotkin's surreal text adventure.
developer: Andrew Plotkin
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
      PowerPC slice and printed the opening text. Typing look and pressing Return produced
      a parser response. A matched control without typing was identical until the input
      and then differed by 20163 pixels. An exact replay matched both checkpoints,
      with zero exhausted frames. The MaxZip status window renders as a solid black bar.
      Saves, longer play and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5435
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: cc1a0a22 browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive. Scripted
      ordinary input typed LOOK and Return; the story window showed the response.
      Twenty-second samples reached 59.99, 61.01 and 60.00 guest ticks per second, with no
      console errors. This approves bounded browser launch; saves, longer play and the blank
      status bar remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5435
runtime:
  executable_path: So Far
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: caa9a719de0b80149d0faf2da8e6554373c844712c6885b3dbcc370efed6aa38
    size_bytes: 406217
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/so-far-r6.hqx
    - https://github.com/benletchford/systemless/issues/5435
    license: Original free distribution notice
    rights_holder: Andrew Plotkin
    permission: >-
      So Far is copyright 1996 by Andrew Plotkin. The author's Info-Mac submission
      says it may be copied, distributed, and played freely. Preserve the complete
      unchanged archive.
    notes: >-
      Stand-alone Macintosh application built with Andrew Plotkin's MaxZip Z-machine
      interpreter; the archive contains only the application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0c5d1a48b595f297456b026adfad89960823c6f0cf95fc376ac3ac269103c959
    size_bytes: 8414
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5435
    permission: >-
      Fresh gameplay capture from the permitted unchanged original package. The story
      text remains its authors' property.
    notes: >-
      Native PowerPC MaxZip story window after the first command, cropped to the
      window.
references:
- https://github.com/benletchford/systemless/issues/5435
---

## Read, then type

![So Far (Release 6) story window](https://assets.systemless.org/catalogue/media/sha256/0c/0c5d1a48b595f297456b026adfad89960823c6f0cf95fc376ac3ac269103c959.png)

Step from a stuffy theatre into a series of strange, shifting worlds in Andrew Plotkin's surreal text adventure.

Type commands such as LOOK, EXAMINE DOOR or GO NORTH and press Return. Type HELP
or ABOUT for the game's own instructions.
