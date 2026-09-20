---
id: gnu-chess-3
kind: game
title: GNU Chess 3.0
summary: >-
  Play a complete game of chess against the GNU engine in Airy André's
  original Macintosh interface.
developer: Free Software Foundation and Airy André
publisher: Free Software Foundation
year: 1991
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
    systemless_version: "0.42.4"
    architecture: 68k
    environment: >-
      Deterministic headless gameplay run from the original GNU Chess 3.0
      Macintosh archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2274
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/GNU%20Chess%203.0.sit
    download_page: https://www.vintageapplemac.com/software/games/g
    expected_sha256: 2cb600b375a44cad2cc0b212133072056484953212b958e43751fb28499a547f
    expected_size: 125350
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/g
    - https://www.vintageapplemac.com/files/games/GNU%20Chess%203.0.sit
    license: GNU Chess General Public License
    rights_holder: Free Software Foundation and Airy André
    permission: >-
      The included GNU Chess General Public License expressly permits copying
      and redistribution of executable code when the corresponding source and
      license accompany it. This unchanged distribution includes the complete
      Macintosh source tree, its resource source, documentation, licence, book
      data and executable together in the same archive.
    notes: >-
      Unchanged 125,350-byte StuffIt archive, SHA-256
      2cb600b375a44cad2cc0b212133072056484953212b958e43751fb28499a547f.
      The included README identifies this as the first Macintosh version, based
      on GNU Chess 3.00, and credits Macintosh porter Airy André.
references:
- https://www.vintageapplemac.com/software/games/g
---

## The GNU engine on a classic Macintosh board

GNU Chess 3.0 brings the Free Software Foundation's chess engine to a native
Macintosh interface. Airy André's 1991 port presents a resizable monochrome
board alongside separate information and move-list windows, with mouse-driven
piece movement and menus for player control, timing and display options.

The compact application supports human-versus-computer and computer-controlled
play, configurable clocks, saved games, move history, board reversal and an
opening book. Its deliberately spare interface keeps the board readable while
exposing the engine's analysis and elapsed time in their own window.

## Preserved with the exact Macintosh source

The original archive is unusually complete: the runnable 68K application sits
beside its opening book, documentation, GNU Chess General Public License and the
full Macintosh C and resource source used to build it. Keeping the complete
package intact preserves both the program and the corresponding-source rights
granted to every recipient.

Systemless was tested beyond launch. A deterministic run opened the board and
entered the legal move e2–e4; the board updated and the application continued
running without a trap, crash or rendering failure.
