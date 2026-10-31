---
id: colossal-cave-adventure
kind: game
title: Adventure (Colossal Cave, Release 3)
summary: >-
  Explore the original Colossal Cave, with its treasures, dwarves and twisty
  little passages, in Graham Nelson's Inform port of the first text adventure.
developer: Will Crowther and Don Woods
year: 1977
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
      and then differed by 1885 pixels. An exact replay matched both checkpoints, with
      zero exhausted frames. The MaxZip status window renders as a solid black bar.
      Saves, longer play and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5441
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 21bc3f3d browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive. Scripted
      ordinary input typed LOOK and Return. Twenty-second samples reached 60.02, 60.03 and
      59.99 guest ticks per second, with no console errors. This approves bounded
      browser launch; saves, longer play and the blank status bar remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5441
runtime:
  executable_path: Adventure
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 3241b4bb6e18237b99d2ada629c78aace4dee8262dbcb6e84fa3619e604c7d4b
    size_bytes: 270174
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/adventure-30.hqx
    - https://github.com/benletchford/systemless/issues/5441
    license: Original free distribution notice
    rights_holder: Graham Nelson and David M. Baggett
    permission: >-
      The Info-Mac submission says this Inform port is freeware, derived from David
      M. Baggett's 1993 TADS reconstruction, which is copylefted under the GNU General
      Public License. Preserve the complete unchanged archive.
    notes: >-
      Stand-alone Macintosh application built with Andrew Plotkin's MaxZip Z-machine
      interpreter; the archive contains only the application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c487712ede93dfded467a78328cf5eb5274a8a588e4cff7d9e22fc72df2cf2cf
    size_bytes: 4971
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5441
    permission: >-
      Fresh gameplay capture from the permitted unchanged original package. The story
      text remains its authors' property.
    notes: >-
      Native PowerPC MaxZip story window after the first command, cropped to the
      window.
references:
- https://github.com/benletchford/systemless/issues/5441
---

## Read, then type

![Adventure (Colossal Cave, Release 3) story window](https://assets.systemless.org/catalogue/media/sha256/c4/c487712ede93dfded467a78328cf5eb5274a8a588e4cff7d9e22fc72df2cf2cf.png)

Explore the original Colossal Cave, with its treasures, dwarves and twisty little passages, in Graham Nelson's Inform port of the first text adventure.

Type commands such as LOOK, EXAMINE DOOR or GO NORTH and press Return. Type HELP
or ABOUT for the game's own instructions.
