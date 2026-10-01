---
id: pattris-plus-plus
kind: game
title: Pattris++
summary: Drop and rotate blocks, with a one-cell filler piece to shoot into gaps.
developer: Valeri Marcello
publisher: Valeri Marcello
year: 1997
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
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened its unregistered 68K build. Dismissed the
      shareware notice; a falling piece appeared and the documented O key moved it
      left on the board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3730
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once. Dismissed the shareware notice; the
      board ran live and O moved the falling piece left. A five-second active-board
      sample measured 60.0 host frames/s, 60.2 guest ticks/s, a 14.1 ms maximum frame, and
      a 131 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3730
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: c1e761fff1faeed0d629efbfc5a4d9df4f5128ff341e65a279cdf464772d47ec
    size_bytes: 99031
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/pattris.hqx
    rights_holder: Valeri Marcello
    permission: >-
      Valeri Marcello submitted this unregistered shareware release to Info-Mac for
      public distribution. The bundled ReadMe describes the 100-second unregistered play
      limit and $5 registration option, with no further distribution restriction. This
      entry uses the complete unchanged archive.
    notes: >-
      Original 99,031-byte BinHex/StuffIt archive, SHA-256
      c1e761fff1faeed0d629efbfc5a4d9df4f5128ff341e65a279cdf464772d47ec.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 317fbea9f40225b22583bf23852013208a47603228ebd8569883c7a422605bb7
    size_bytes: 4858
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3730
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware release for this
      catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 272-by-361 game-content crop at (280,294) from a 1280-by-900 Chrome
      screenshot after moving the falling piece left.
references:
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/pattris.hqx
---

## Fill the gaps

![A falling block in Pattris++](https://assets.systemless.org/catalogue/media/sha256/31/317fbea9f40225b22583bf23852013208a47603228ebd8569883c7a422605bb7.png)

Move left with **O**, right with **P**, rotate with **D**, and drop with **C**.
Press Space to shoot the special one-cell filler piece. The unregistered
shareware version limits each game to 100 seconds.
