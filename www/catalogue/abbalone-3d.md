---
id: abbalone-3d
kind: game
title: Abbalone 3D 1.0
summary: >-
  Push opposing marbles off a three-dimensional board in this Macintosh
  adaptation of Abalone.
developer: Anomic Software
year: 1996
architectures:
- 68k
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: Combined 2353ccaa and PRs 5388, 5389, 5391
    architecture: ppc
    environment: >-
      The original Info-Mac archive ran natively at depth 8. After the shareware
      screen, a red marble was dragged to an adjacent empty space; four measured
      before-and-after pixel assertions passed at frontend tick 1568 with zero exhausted frames.
      The 68K route reached the initial board but its gameplay has not been
      established. Browser performance, full games, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5401
runtime:
  executable_path: Abbalone 3D US 1.0/Abbalone 3D US
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: b1f4d77e09cb9aee382c928fbfc09b213e80a0ec820e44d7518d1b282c133a9a
    size_bytes: 1047025
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/abbalone-3d.hqx
    - https://github.com/benletchford/systemless/issues/5401
    license: Original noncommercial complete-package distribution notice
    rights_holder: Anomic Software
    permission: >-
      The original Read Me permits copies of the complete package on FTP and other
      Internet services, excluding MSN services. It prohibits selling the distribution or
      including it in commercial products. Preserve the complete unchanged original
      archive, guide, registration application and notices. The included Register
      application separately permits unmodified distribution.
    notes: >-
      The original is shareware with a one-month trial. The Read Me gives conflicting
      US$20 and US$30 registration prices; retain the original wording and
      registration program without modification. The original application contains 68K and PowerPC
      routes.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3b47c66504180dd5ee726f00d387ca615c1e4a3ace0da0a0b2129585cbe00430
    size_bytes: 45588
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5401
    permission: >-
      Original gameplay capture from the permitted unchanged package, cropped to the
      game board.
    notes: >-
      Native PowerPC board after a legal player move; excludes the menu bar, window
      frame, desktop and emulator interface.
references:
- https://github.com/benletchford/systemless/issues/5401
---

## Push the marbles

![Abbalone 3D board after a player move](https://assets.systemless.org/catalogue/media/sha256/3b/3b47c66504180dd5ee726f00d387ca615c1e4a3ace0da0a0b2129585cbe00430.png)

Move your marbles across the board and push your opponent's marbles over the
edge. Play against the Macintosh or another player. The original shareware
allows one month of use before registration is required.
