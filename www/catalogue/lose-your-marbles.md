---
id: lose-your-marbles
kind: game
title: Lose Your Marbles!
summary: >-
  Fill a grid with marbles without repeating a colour in any row, column or
  diagonal.
developer: Mark Pilgrim
publisher: MerriMac Software Group
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 155f719580ed6cc330efae22f548b9fc87158df7
    architecture: 68k
    environment: >-
      Unchanged original 1.0 BinHex/Compact Pro archive, explicit 68K and 8-bit
      native display. Ordinary OK accepts the optional name prompt; Cmd-N opens a 5x5 board.
      Number keys place two distinct marbles, Right Arrow moves the selection, and
      Space erases the second while preserving the first. Eight measured assertions pass
      at 1032 frontend ticks. A separate ordinary mouse/number-key replay completes a
      five-colour solution and the game's minimum-piece win screen; three completion
      assertions pass at 1954 frontend ticks. Captures inspected, zero exhausted frames.
      Browser input, larger boards, save/load and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4547
runtime:
  executable_path: Lose Your Marbles! 1.0.cpt/Lose Your Marbles! 1.0
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 00c7d6ff2e8b144520d80fc9ca924bb2fd87b3444bd8a402302477035bbb0118
    size_bytes: 122888
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/losemarb/index.html
    - https://github.com/benletchford/systemless/issues/4547
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Original game TEXT 640 explicitly licenses this release under GNU GPL version 2
      or later and retains the copyright and warranty notice. Keep the unchanged
      original complete package and bundled licence. The matching complete original source
      archive is included as a managed supplement.
    notes: >-
      Original-format receipt MD5 fc6f004561627baa7662fcb9de2184e2 matches. The
      original resource-only game has five CODE resources and no PPC slice. Production fork
      parity and browser launch approval remain pending.
- id: source-code
  role: supplement
  format: hqx
  source:
    type: sha256
    sha256: a5b2f95a39a1ca64e4ecf8fea8525e32ebd7b6aa7619e13986efd916932a9663
    size_bytes: 250048
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/losemarb/index.html
    - https://github.com/benletchford/systemless/issues/4547
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Corresponding original THINK C project, resources and source. All 63 C and
      header files carry explicit GPL notices. Keep the entire unchanged archive available
      alongside the binary package; retain the original licence.
    notes: >-
      Original receipt MD5 5e3d24737675df25c952f2c5c3b0798e matches. Independent
      expansion yields 66 files including project, resource file and licence. Binary and
      source licence documents byte-match at 17982 bytes.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0b0006899c651372b4e1320702a117c1151898a367071f88eed46c2b1a0ac820
    size_bytes: 1592
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4547
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Original Systemless capture from the exact unchanged licensed game archive.
      Underlying game artwork is distributed under the original game's GPL terms.
    notes: >-
      Native two-marble gameplay, cropped to the complete game grid. Excludes Classic
      Mac menu bar, window framing, emulator and host interface.
references:
- https://www.gryphel.com/c/sw/games/losemarb/index.html
- https://github.com/benletchford/systemless/issues/4547
---

Fill each square with a marble so that no row, column or diagonal repeats a
colour. Try to solve the board using as few different colours as possible.

Type a number from 0 to 9 to place a marble. Move with the arrow keys or select
a square with the mouse. Space erases the selected marble.

![Two different marbles on the game grid](https://assets.systemless.org/catalogue/media/sha256/0b/0b0006899c651372b4e1320702a117c1151898a367071f88eed46c2b1a0ac820.png)

This original release is free software under GNU GPL version 2 or later.
[Download its complete original source](https://assets.systemless.org/catalogue/objects/sha256/a5/a5b2f95a39a1ca64e4ecf8fea8525e32ebd7b6aa7619e13986efd916932a9663.hqx).

Browser launch approval is pending.
