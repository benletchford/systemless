---
id: snake-byte
kind: game
title: Snake Byte 1.2
summary: Guide a growing snake toward apples and away from walls and its tail.
developer: Pierre-Luc Paour
publisher: Pierre-Luc Paour
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged author-submitted Info-Mac freeware archive opened in 68K
      Systemless. After its About box and high scores, File > New Game reached a live board. In
      matched replays, the snake continued upward without input while holding the
      default left key, 4, turned it left.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3772
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once.
      After dismissing About and selecting File > New Game, the snake moved and the
      lives counter changed on the live board. A real browser 4-key press turned the snake
      left after play resumed. A five-second active sample measured about 60 host
      frames and 60 guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3772
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: d567078dfd9d8a85ecab5495a9e4714e6b47192b4c41b683088caab787f93dcd
    size_bytes: 649131
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/snake-byte-12.hqx
    rights_holder: Pierre-Luc Paour
    permission: >-
      The game's embedded FreeWare Notice permits free distribution when Snake Byte
      is distributed freely and unmodified. The author-submitted Info-Mac archive is
      preserved unchanged, including its embedded help and notice.
    notes: >-
      Original 649,131-byte BinHex/StuffIt archive, SHA-256
      d567078dfd9d8a85ecab5495a9e4714e6b47192b4c41b683088caab787f93dcd.
      The promoted public object was fetched and confirmed byte-for-byte identical
      to the original download on 2026-10-02.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d757e9948ffa1b5bb342a3cc3cd16ffb6b908181683881355c6ad8511af71a61
    size_bytes: 68218
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3772
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware game. Underlying
      artwork remains its owner's property.
    notes: >-
      Exact 626-by-438 game-content crop at (87,100) from an 800-by-600 Systemless
      framebuffer after the default left-key turn; the live board, apple, snake, and
      score panel remain visible. The promoted public PNG matched its 68,218-byte
      SHA-256 source on 2026-10-02.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/snake-byte-12.hqx
---

## Eat apples, avoid your tail

![Snake Byte after a left turn](https://assets.systemless.org/catalogue/media/sha256/d7/d757e9948ffa1b5bb342a3cc3cd16ffb6b908181683881355c6ad8511af71a61.png)

Choose **New Game** from the **File** menu. The snake moves on its own; press
**4** to turn left or **6** to turn right under the default two-key controls.
Eat apples to grow and clear each level, and avoid walls and your own tail.
