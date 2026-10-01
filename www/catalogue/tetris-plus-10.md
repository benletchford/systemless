---
id: tetris-plus-10
kind: game
title: Tetris Plus 1.0
summary: Match three colours as falling blocks reshape the board.
developer: Greg Fudala
publisher: Greg Fudala
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.73.0 + release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac package opened its 68K application. Game > New Game
      reached a live falling-block board; the piece descended over time, and the documented
      keypad 4 moved it left. The release browser loaded the original archive once,
      entered the live board through Game > New Game, and showed keypad 4 move the piece
      left and keypad 6 move it right.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3828
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: e98f8a0f0472292821d31421c048e92a3cd3e07e1a54aa88bbc91220613e68f0
    size_bytes: 668258
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/tetris-plus-10.hqx
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    rights_holder: Greg Fudala
    permission: >-
      The bundled README and author-submitted Info-Mac abstract permit anyone to
      distribute the unaltered freeware package by any means without profit, including
      CD-ROM distribution.
    notes: >-
      Original BinHex/StuffIt package with the application, music, and README;
      SHA-256 e98f8a0f0472292821d31421c048e92a3cd3e07e1a54aa88bbc91220613e68f0.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 47b4913c7da419783b7afb385a9ae954a598e3f330b571b059d057f5a9f0c8a0
    size_bytes: 18199
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3828
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: Content-only active-board crop; emulator framing is excluded.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/tetris-plus-10.hqx
---

## Match colours as pieces fall

![Tetris Plus active board](https://assets.systemless.org/catalogue/media/sha256/47/47b4913c7da419783b7afb385a9ae954a598e3f330b571b059d057f5a9f0c8a0.png)

Choose **Game → New Game** to start. Use keypad **4** and **6** to move left
and right, keypad **2** to descend, and keypad **9** and **8** to rotate.
The Game menu also has **Configure Keys** if your keyboard lacks a keypad.
Match three or more blocks of the same colour and clear the flash blocks.
