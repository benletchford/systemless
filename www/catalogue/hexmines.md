---
id: hexmines
kind: game
title: Hexmines
summary: Find a safe path through a minefield on a hexagonal grid.
developer: Ingemar Ragnemalm
year: 2000
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 0f4ee1ae9fa59ab6d6be0c511aee0c5d920e4142
    architecture: 68k
    environment: >-
      Bounded native v0.93.0 replay of the complete original 2.0.4 BinHex/Compact Pro
      archive, exact nested executable, explicit 68K slice and 800-by-600/8-bit display.
      Return accepts the original mine-count dialog. Ordinary clicks reveal two
      distinct numbered hexes. Inspected captures and a fresh six-pixel assertion
      replay pass at 926 frontend / 1526 guest ticks with zero exhausted frames.
      Both forks of both original files independently match unar extraction.
      This original application has four CODE resources and no PPC cfrg or PEF.
      Browser/public input, marking, completed paths, wins, settings, saves and
      audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4454
runtime:
  executable_path: Hexmines 2.0.4.cpt/Hexmines 2.0.4
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.lysator.liu.se/pub/mac/games/Hexmines-204.hqx
    expected_sha256: 5bad2fac69ad0e3b2006853bc3cf3bdb9f45010088e38f63e99ee4a29f8058cd
    expected_size: 79058
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.lysator.liu.se/pub/mac/games/Hexmines-204.hqx
    - https://www.lysator.liu.se/~ingemar/games.html
    - https://github.com/benletchford/systemless/issues/4454
    license: Original noncommercial freeware distribution grant
    rights_holder: Ingemar Ragnemalm
    permission: >-
      The original Hexmines 2.0.4 docs explicitly permit free use and redistribution.
      Sale and commercial distribution require the author's written permission;
      inclusion on shareware CDs requires sending him a complimentary copy.
      This free preservation distribution retains the complete unchanged original
      package, documentation and copyright notices. No commercial-distribution
      or modern source-code licence is inferred.
    notes: >-
      Original 79058-byte Hexmines-204.hqx, not repacked or converted. Production
      decoding yields the original application (empty data fork, 99013-byte resource
      fork) and docs (7546-byte data fork, 612-byte resource fork). Independent unar
      extraction matches all four forks. The version 2.0.4 docs identify its 2000
      copyright and five-year update; the game originated in 1991. Original 68K-only
      application; a native PPC version is not present in this package.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/hexmines/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4454
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Lossless crop of the inspected native guest frame to the original game content
      surface. Shows the hexagonal minefield and two numbered hexes after ordinary
      clicks, without host UI, window title or Classic menu bar. No redraw.
references:
- https://www.lysator.liu.se/~ingemar/games.html
---

![Hexmines minefield after revealing two numbered hexes](incoming/hexmines/gameplay.png)

Accept the mine count to start. Click a hex to step on it; its number tells you
how many neighboring hexes contain mines. Find a safe path through the minefield.
The original game also offers a mode for finding all mines, selectable settings
and help boxes.

This is the complete original **Hexmines 2.0.4** freeware package, including its
documentation and noncommercial redistribution terms. This release is 68K-only.

Bounded native testing verifies starting the board and revealing numbered hexes.
Browser and public gameplay, marking, complete paths, wins, settings, saves and
audio remain unverified.
