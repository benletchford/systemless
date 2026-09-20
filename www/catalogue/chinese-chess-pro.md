---
id: chinese-chess-pro
kind: game
title: Chinese Chess Pro 1.0.1
summary: >-
  Play Chinese chess against the computer or another player in Tie Zeng's
  native Macintosh Chinese Chess Pro package.
developer: Tie Zeng
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: >-
      Deterministic gameplay run from the complete unchanged Chinese Chess Pro
      1.0.1 archive, using the Chinese Chess Pro Color 68k application path
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2318
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Chinese%20Chess%20Pro%201.0.1.sit
    download_page: https://www.vintageapplemac.com/software/games/c/
    expected_sha256: a9b0f0f8686d9f104fef15fa3d1994cdf5cec502e6ed02d5da902d35a769fb9f
    expected_size: 242056
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/c/
    - https://www.vintageapplemac.com/files/games/Chinese%20Chess%20Pro%201.0.1.sit
    license: Chinese Chess Pro 1.0.1 bundled Distribution permission
    rights_holder: Tie Zeng
    permission: >-
      The bundled About Chinese Chess Pro document identifies the complete
      package (Chinese Chess Pro, Chinese Chess Pro Sr and Chinese Chess Pro
      Color) and states: “You may freely distribute this package without any
      chang…” and “you may also upload the package to any commercial or
      non-profit bulletin boards. This software may not be included in any
      software packages sold for profit without explicit written permission from
      the author.” The entry therefore references the complete, unchanged
      archive for non-profit distribution with all original files; the bundled
      MWII text resource visibly truncates the first sentence after “chang”.
    notes: >-
      Unchanged 242,056-byte StuffIt archive, pinned by SHA-256
      a9b0f0f8686d9f104fef15fa3d1994cdf5cec502e6ed02d5da902d35a769fb9f. The
      complete package retains all three 68k applications, cchess.book,
      Introduction to Chinese Chess, The Board and About Chinese Chess Pro.
references:
- https://www.vintageapplemac.com/software/games/c/
- https://www.vintageapplemac.com/files/games/Chinese%20Chess%20Pro%201.0.1.sit
- https://github.com/benletchford/systemless/issues/2318
---

## Chinese chess on the Macintosh

Chinese Chess Pro 1.0.1 brings the Chinese chess board game to a native
Macintosh interface. The complete distribution includes the standard, small
screen and color applications, an opening book, board documentation and the
bundled About file.

## Preserved with its distribution terms

The bundled Distribution text says: “You may freely distribute this package
without any chang…” and “you may also upload the package to any commercial or
non-profit bulletin boards. This software may not be included in any software
packages sold for profit without explicit written permission from the author.”
The bundled MWII text resource visibly truncates the first sentence after
“chang”; this catalogue entry points to the unchanged 242,056-byte archive and
keeps every original package component together.

## A real move on the live board

Systemless reaches the live board from the exact archive using the verified
Chinese Chess Pro Color 68k path. A deterministic run selected a red soldier at
`(h=22,v=265)` and moved it legally one rank forward to `(h=22,v=231)`; the
before/after frames show the piece changing squares. This verifies gameplay
beyond launch and menu navigation. See [issue #2318](https://github.com/benletchford/systemless/issues/2318)
for the reproducibility record.
