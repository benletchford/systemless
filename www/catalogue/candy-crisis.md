---
id: candy-crisis
kind: game
title: Candy Crisis 1.2
summary: >-
  Stack and match falling candies to send garbage to your rival in John Stiles'
  colourful falling-block puzzle game.
developer: John Stiles
year: 1997
architectures:
- ppc
default_architecture: ppc
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 44853346 deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged original Mac OS 9 release ran natively at depth 8. After Not Yet
      on the shareware notice, which the open-source re-release keeps unmodified, the
      main menu appeared. Hovering and clicking One Player Game started Stage 1 against
      the computer. Three D presses moved the falling candy pair to the right. A
      matched control without the keys was identical until the input and then differed by
      1,989 pixels. An exact replay matched both checkpoints, with zero exhausted frames.
      Part of the shareware notice text is covered by a black block, and the menu mode
      labels are dim. Full matches, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4789
runtime:
  executable_path: Candy Crisis/Candy Crisis
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: fe3295893e58dc4ee7e6339ba36083b507d8334dff7d51b2370e61320d51c9b9
    size_bytes: 7487452
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://sourceforge.net/projects/candycrisis/files/Candy%20Crisis%201.2%20for%20Mac%20OS/Full%20Release/
    - https://candycrisis.sourceforge.net/
    - https://github.com/benletchford/systemless/issues/4789
    license: GPL-2.0 (game); LGPL (bundled MikMod library)
    rights_holder: John Stiles
    permission: >-
      The author re-released Candy Crisis 1.2 as free and open-source software under
      the GPLv2. Corresponding source is published beside the binary
      (CandyCrisisSourceCode_MacOSX.zip, SHA-256
      5d6a93c17cc1849d9357b373fae51541f383b7ec8b00839ca1eb1d833d091208). The bundled MikMod 3.2.0b2 library is LGPL, and its upstream source is
      published as libmikmod-3.2.0-beta2.tar.gz (SHA-256
      857b66ef04d695f70414188b985e08b04f1f62cc250d72a43d0e0609dfbdba03). The unchanged package and its notices are
      preserved.
    notes: >-
      PowerPC only. The README explains that the old shareware notice remains in the
      unmodified program; no registration is required.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2746b10e196d17e8ba1bec129ec4f3c61e413b12adee1f06357bba92a08c9b6e
    size_bytes: 433755
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4789
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native PowerPC Stage 1 after the player moves the falling pair right.
references:
- https://candycrisis.sourceforge.net/
- >-
  https://sourceforge.net/projects/candycrisis/files/Candy%20Crisis%201.2%20for%20Mac%20OS/
- https://mikmod.sourceforge.net/old_releases.html
- https://github.com/benletchford/systemless/issues/4789
---

## Crush the candy

![Candy Crisis Stage 1](https://assets.systemless.org/catalogue/media/sha256/27/2746b10e196d17e8ba1bec129ec4f3c61e413b12adee1f06357bba92a08c9b6e.png)

Drop pairs of candies into your well and match four of a colour to clear them.
Chain reactions send junk to your opponent. Beat a series of computer rivals in
the one-player game, challenge a friend, or try Solitaire Crisis.

Click **Not Yet** on the old shareware notice; this free release needs no
registration. Choose **One Player Game**. Player 1 uses A and D to move, S to
drop and W to rotate.
