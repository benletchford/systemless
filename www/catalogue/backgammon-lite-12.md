---
id: backgammon-lite-12
kind: game
title: Backgammon Lite v1.2
summary: Move checkers across a classic backgammon board with a friend and physical dice.
developer: Alesh Slovak
publisher: Alesh Slovak
year: 1999
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.74.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac package opened its 68K board. Dragging the top-left red
      checker to the second lower point moved it and changed the board. The release
      browser fetched the archive once and reproduced the same drag.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3912
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 452217fae86aeeee69b1fc7756d59686b96dd0bd7f943960991f03b2a851d0ab
    size_bytes: 41279
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/backgammon-lite-12.hqx
    rights_holder: Alesh Slovak
    permission: >-
      The bundled ReadMe permits distribution of the unmodified freeware program when
      its original documentation is included. This archive preserves both the
      application and ReadMe.
    notes: >-
      Original 41,279-byte Info-Mac archive; SHA-256
      452217fae86aeeee69b1fc7756d59686b96dd0bd7f943960991f03b2a851d0ab.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 44600443a88c54ea8e89af982a3539cfcc66b0852cb79c415e594fa189717f7d
    size_bytes: 51234
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3912
    permission: Fresh Systemless gameplay capture from the unchanged original package.
    notes: >-
      640-by-480 board crop at (80,60) from an 800-by-600 deterministic framebuffer
      after dragging a red checker.
references:
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/backgammon-lite-12.hqx
---

## A board for two

![Backgammon Lite board after moving a red checker](https://assets.systemless.org/catalogue/media/sha256/44/44600443a88c54ea8e89af982a3539cfcc66b0852cb79c415e594fa189717f7d.png)

Play with a friend and a **physical pair of dice**. Click and drag a checker to
move it to another point. This edition provides the board and checker movement;
its ReadMe asks players to supply dice and know the rules. Press **Escape** to
quit.

The original archive includes the application and its ReadMe. The author
permits redistribution when the package remains unmodified and the ReadMe is
included.
