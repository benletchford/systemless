---
id: tic-tac-toe
kind: game
title: Tic Tac Toe
summary: Take turns placing richly illustrated X and O pieces on a three-by-three board.
developer: Gei-Tai Lin
publisher: Gei-Tai Lin
year: 1998
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened the 68K game. File > New Game reset the
      board. Clicking the center square placed an X; clicking the upper-left square on
      the next turn placed an O.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3753
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once.
      Real browser input chose File > New game, placed X in the center square, then
      placed O in the upper-left square. A five-second sample ran at about 60 host frames
      and 60 guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3753
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 14f4e0a64dfdc3f6e4ef82ccb3c127f9a3a6aa7a14e7f515e54ddeddd401ba6c
    size_bytes: 310927
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=10781
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/tic-tac-toe.hqx
    rights_holder: Gei-Tai Lin and Tyler Esselstrom
    permission: >-
      The author's bundled Readme explicitly permits free distribution if the game
      remains unmodified and the Readme accompanies it. This is the complete unchanged
      archive, including the game, music, and Readme.
    notes: >-
      Original 310,927-byte BinHex/StuffIt archive, SHA-256
      14f4e0a64dfdc3f6e4ef82ccb3c127f9a3a6aa7a14e7f515e54ddeddd401ba6c.
      The promoted public object was fetched back and matched this hash and size.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 184aa8d2ab4a2d0df9dc5da127895b758fdfdb5582767cf2cf63ac3ce0e63c89
    size_bytes: 47993
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3753
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware release for this
      catalogue entry. Underlying artwork remains its owners' property.
    notes: >-
      Exact 302-by-300 board-content crop at (40,42) from an 800-by-600 Systemless
      framebuffer after an X and O were placed. The crop excludes the Classic Mac menu
      bar and window frame. The promoted public object was fetched back and matched its
      submitted hash and 47,993-byte size.
references:
- https://info-mac.org/viewtopic.php?t=10781
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/tic-tac-toe.hqx
---

## Place three in a row

![An X and O placed on the Tic Tac Toe board](https://assets.systemless.org/catalogue/media/sha256/18/184aa8d2ab4a2d0df9dc5da127895b758fdfdb5582767cf2cf63ac3ce0e63c89.png)

Choose **New game** from the **File** menu, then click an empty square to place
your piece. The **Computer Intelligence** menu offers three difficulty levels.
