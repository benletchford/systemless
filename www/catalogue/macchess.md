---
id: macchess
kind: game
title: MacChess 2.5.1
summary: >-
  Play chess against Wim van Beusekom's computer opponent with an opening book
  and move analysis.
developer: Wim van Beusekom
publisher: Wim van Beusekom
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac 68K archive opened the standard board and its opening
      book. Dragging the white e-pawn from e2 to e4 updated the board; the computer then
      replied with a knight move to f6.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3756
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once. A
      real browser mouse drag moved the white e-pawn from e2 to e4, and the computer
      replied Nf6. A five-second active-board sample ran at about 60 host frames and 60
      guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3756
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: e9df9a62ba9d250010285f0149f5b2526b37bba93923fc20e8db958ccc5920cf
    size_bytes: 580372
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?p=4684
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/brd/mac-chess-251-68k.hqx
    rights_holder: Wim van Beusekom
    permission: >-
      The bundled MacChess 2.5 manual identifies the game as freeware and encourages
      online services to distribute the unmodified package. It prohibits distribution
      for profit or on CD-ROM without permission. This entry preserves the complete
      original package with its manual.
    notes: >-
      Original 580,372-byte BinHex/StuffIt archive, SHA-256
      e9df9a62ba9d250010285f0149f5b2526b37bba93923fc20e8db958ccc5920cf.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ce80d4d18e4b6aa041eaf658ef259141c14b4f4a396de01c9cd2289570145828
    size_bytes: 7929
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3756
    permission: >-
      Fresh Systemless gameplay capture from the author's freeware release for this
      catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 257-by-256 board-content crop at (15,38) from an 800-by-600 Systemless
      framebuffer after white played e4 and black replied Nf6. The crop excludes the
      Classic Mac menu bar and window frames.
references:
- https://info-mac.org/viewtopic.php?p=4684
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/brd/mac-chess-251-68k.hqx
---

## Play a game of chess

![MacChess board after e4 and Nf6](https://assets.systemless.org/catalogue/media/sha256/ce/ce80d4d18e4b6aa041eaf658ef259141c14b4f4a396de01c9cd2289570145828.png)

Drag a piece to its destination square. MacChess responds as the computer
opponent and records moves in the adjacent window. Use the **Level** menu to
adjust its thinking time.
