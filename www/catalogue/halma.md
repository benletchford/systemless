---
id: halma
kind: game
title: Halma
summary: >-
  Move nine pieces to the opposite corner using steps and chains of jumps, aiming
  for the fewest moves.
developer: Mark Pilgrim
publisher: MerriMac Software Group
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 2a4c8e13267819e66405b140bde8cd00461db16f
    architecture: 68k
    environment: >-
      Exact original 1.2 BinHex/Compact Pro archive, explicit 68K and 8-bit display.
      Ordinary OK accepts the optional name prompt, then Cmd-N opens the original 6x4
      board. Ordinary mouse selection and a legal diagonal single step move a piece to
      an empty cell; inspected move count advances from 0 to 1. A subsequent legal jump moves a piece over its neighbour without removing
      that neighbour. Seven measured step/jump assertions pass at 1092 frontend /
      1692 guest ticks with zero exhaustion.
      Full-board completion, save/load, audio and browser launch qualification remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4553
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 4746a9751103b1fe17320dc32991680ef8d1ffa3
    architecture: 68k
    environment: >-
      Optimized Chrome preview with local frontend responses under the production
      origin. Actual managed game fetch untouched, HTTP 200, original SHA-256 and
      107312-byte body verified. Isolated 800x600 68K worker/WebGL, 25 MHz and max
      two ticks per paint. Ordinary OK, Command-N and mouse input open the original
      6x4 board and perform a legal step plus jump, preserving the jumped piece.
      Ten measured before/after pixel checks pass; captures inspected and step
      counter advances 0 to 1. Managed corresponding-source link verified.
      Complete-board solutions, larger boards, save/load and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4553
runtime:
  executable_path: Halma 1.2.cpt/Halma 1.2
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: ea88e1708d2b74efb4fb7bcfacd8a0504d83200268574b486e1124dad34b15be
    size_bytes: 107312
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/halma/index.html
    - https://github.com/benletchford/systemless/issues/4553
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Original game TEXT 640 explicitly licenses this release under GNU GPL version 2
      or later and retains the copyright and warranty notice. Keep the unchanged
      original complete package and bundled licence. The matching complete original source
      archive is included as a managed supplement.
    notes: >-
      Original-format receipt MD5 1bb050cdecb5568d8997cc8688bc52fd matches. The
      original resource-only game has five CODE resources and no PPC slice. Both original
      files/all four forks and every Finder field match independent extraction. Browser
      step/jump input is verified and launch approved.
- id: source-code
  role: supplement
  format: hqx
  source:
    type: sha256
    sha256: 4ded773e6a0ea802a6dc66d72c1685222797f6ecbef2d635c1e4d78f83ac401d
    size_bytes: 234430
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/halma/index.html
    - https://github.com/benletchford/systemless/issues/4553
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Corresponding original THINK C project, resources and source. All 61 C and
      header files carry explicit GPL notices. Keep the entire unchanged archive available
      alongside the binary package; retain the original licence.
    notes: >-
      Original receipt MD5 698f29a0d4729f30cc24361837ba7e8b matches. Independent
      expansion yields 64 files including project, resource file and licence. Binary and
      source licence documents byte-match at 17982 bytes.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 885a8a0cd6d4c77ed86c4fc23318ec254031ac166bf9d1398ee2e9ad9c1abd37
    size_bytes: 1226
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4553
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim
    permission: >-
      Original Systemless capture from the exact unchanged licensed game archive.
      Underlying game artwork is distributed under the original game's GPL terms.
    notes: >-
      Native legal single-step gameplay, cropped to the complete game grid. Excludes
      Classic Mac menu bar, window framing, emulator and host interface.
references:
- https://www.gryphel.com/c/sw/games/halma/index.html
- https://github.com/benletchford/systemless/issues/4553
---

Move all nine pieces from the bottom-left corner to the top-right corner.
Select a piece with the mouse, then click an empty neighbouring square to step,
or jump over another piece into an empty square. Chains of jumps count as one move.

![A legal move on the Halma board](https://assets.systemless.org/catalogue/media/sha256/88/885a8a0cd6d4c77ed86c4fc23318ec254031ac166bf9d1398ee2e9ad9c1abd37.png)

This original release is free software under GNU GPL version 2 or later.
[Download its complete original source](https://assets.systemless.org/catalogue/objects/sha256/4d/4ded773e6a0ea802a6dc66d72c1685222797f6ecbef2d635c1e4d78f83ac401d.hqx).

Browser step and jump input is verified. Complete-board play, larger boards, saving and audio remain unverified.
