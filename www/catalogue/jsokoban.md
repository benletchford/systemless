---
id: jsokoban
kind: game
title: JSokoban
summary: >-
  Push every money bag onto its destination across 85 compact warehouse
  puzzles in Jason Townsend's colourful Macintosh Sokoban.
developer: Jason Townsend
publisher: Jason Townsend
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.42.4"
    architecture: 68k
    environment: >-
      Deterministic headless gameplay run from the complete original JSokoban
      1.0b13 Macintosh archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2279
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/JSokoban_1.0b13.sit
    download_page: https://www.vintageapplemac.com/software/games/j
    expected_sha256: 04452000ec19ec9a271f340e1ba1c936a3e56f3ea0ff70a0a30545060041ad91
    expected_size: 61522
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/j
    - https://www.vintageapplemac.com/files/games/JSokoban_1.0b13.sit
    license: JSokoban freeware non-profit distribution grant
    rights_holder: Jason Townsend and contributing level authors
    permission: >-
      The included JSokoban Doc identifies the game as freeware and permits
      non-profit distribution when the documentation, Extra file and Bug Report
      file are included. This unchanged archive contains the application,
      JSokoban Doc, Extra and Bug Report Form together as required.
    notes: >-
      Unchanged 61,522-byte StuffIt archive, SHA-256
      04452000ec19ec9a271f340e1ba1c936a3e56f3ea0ff70a0a30545060041ad91.
      The documentation identifies the game, excluding its 85 bundled levels,
      as copyright 1994 Jason Townsend.
references:
- https://www.vintageapplemac.com/software/games/j
---

## Eighty-five warehouses to untangle

JSokoban adapts the classic Japanese warehouse puzzle to a native Macintosh
window. Guide the keeper through cramped rooms and push every money bag onto a
marked destination. Bags can only be pushed, never pulled, so one careless move
into a corner can make a level impossible.

The package combines the familiar 50-level set with 35 additional puzzles. It
supports the mouse, arrow keys, numeric keypad and I-J-K-L controls, along with
multi-square movement, push undo, level restart and a built-in demonstration of
the first puzzle. Colour, greyscale and black-and-white displays are supported
from System 6.0.7 onward.

## The complete freeware package

The author's distribution terms require JSokoban to remain non-profit and to
travel with its documentation, Extra levels and bug-report form. This entry
therefore preserves the original archive intact rather than separating the game
from the supporting files that explain its controls, history and licence.

Systemless was tested beyond startup: a deterministic run displayed the puzzle
board and moved the warehouse keeper one tile using mouse input, with the board
updating normally and no trap, crash or rendering failure.
