---
id: spider-and-web
kind: game
title: Spider and Web (Release 4)
summary: >-
  Be interrogated about the alley and the door you were caught behind in Andrew
  Plotkin's celebrated spy text adventure.
developer: Andrew Plotkin
year: 1998
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
      and then differed by 22727 pixels. An exact replay matched both checkpoints,
      with zero exhausted frames. The MaxZip status window renders as a solid black bar.
      Saves, longer play and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5434
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: cc1a0a22 browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive. Scripted
      ordinary input typed LOOK and Return; the story window showed the response.
      Twenty-second samples reached 60.02, 60.00 and 60.01 guest ticks per second, with no
      console errors. This approves bounded browser launch; saves, longer play and the blank
      status bar remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5434
runtime:
  executable_path: Spider And Web
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 8d8881ca30e44e2fc4718fafb4811588d1c220df3aa8c0dd1f118db9f21db885
    size_bytes: 388084
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/spider-and-web.hqx
    - https://github.com/benletchford/systemless/issues/5434
    license: Original free distribution notice
    rights_holder: Andrew Plotkin
    permission: >-
      Spider And Web is copyright 1997-8 by Andrew Plotkin. The author's Info- Mac
      submission says it may be copied, distributed, and played freely. Preserve the
      complete unchanged archive.
    notes: >-
      Stand-alone Macintosh application built with Andrew Plotkin's MaxZip Z-machine
      interpreter; the archive contains only the application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f698131c64e0010c91a5461e3b235ca1279e8f0066565c1a36d877bfea00e668
    size_bytes: 10755
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5434
    permission: >-
      Fresh gameplay capture from the permitted unchanged original package. The story
      text remains its authors' property.
    notes: >-
      Native PowerPC MaxZip story window after the first command, cropped to the
      window.
references:
- https://github.com/benletchford/systemless/issues/5434
---

## Read, then type

![Spider and Web (Release 4) story window](https://assets.systemless.org/catalogue/media/sha256/f6/f698131c64e0010c91a5461e3b235ca1279e8f0066565c1a36d877bfea00e668.png)

Be interrogated about the alley and the door you were caught behind in Andrew Plotkin's celebrated spy text adventure.

Type commands such as LOOK, EXAMINE DOOR or GO NORTH and press Return. Type HELP
or ABOUT for the game's own instructions.
