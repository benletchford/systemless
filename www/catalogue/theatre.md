---
id: theatre
kind: game
title: Theatre (Release 2)
summary: >-
  Search a decaying old theatre for your lost pager on a night of horror in
  Brendon Wyber's text adventure.
developer: Brendon Wyber
year: 1995
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: cc1a0a22 deterministic play runner
    architecture: ppc
    environment: >-
      The original Info-Mac package opened its bundled MaxZip interpreter on the
      PowerPC slice and printed the opening text. Typing ook (first key typed before the
      prompt was ready) and pressing Return produced a parser response. A matched control
      without typing was identical until the input and then differed by 38150 pixels.
      An exact replay matched both checkpoints, with zero exhausted frames. The MaxZip
      status window renders as a solid black bar. Saves, longer play and browser
      performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5436
runtime:
  executable_path: Theatre
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 22098a8d83c9011e13d71df5ca3bccbf38eb9e90bf03632ce5ff8d9c2cf14494
    size_bytes: 355159
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/theatre-20.hqx
    - https://github.com/benletchford/systemless/issues/5436
    license: Original free distribution notice
    rights_holder: Cave Rock Software Ltd.
    permission: >-
      THEATRE is copyright 1995 by Cave Rock Software Ltd. The Info-Mac submission
      gives permission for the game to be freely distributed by any and all possible
      means, with no charge except distribution costs. Preserve the complete unchanged
      archive.
    notes: >-
      Stand-alone Macintosh application built with Andrew Plotkin's MaxZip Z-machine
      interpreter; the archive contains only the application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 60115b875da74ae811842dd4c67927b82dd0c4c8d154493400418e07f732b556
    size_bytes: 6380
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5436
    permission: >-
      Fresh gameplay capture from the permitted unchanged original package. The story
      text remains its authors' property.
    notes: >-
      Native PowerPC MaxZip story window after the first command, cropped to the
      window.
references:
- https://github.com/benletchford/systemless/issues/5436
---

## Read, then type

![Theatre (Release 2) story window](https://assets.systemless.org/catalogue/media/sha256/60/60115b875da74ae811842dd4c67927b82dd0c4c8d154493400418e07f732b556.png)

Search a decaying old theatre for your lost pager on a night of horror in Brendon Wyber's text adventure.

Type commands such as LOOK, EXAMINE DOOR or GO NORTH and press Return. Type HELP
or ABOUT for the game's own instructions.
