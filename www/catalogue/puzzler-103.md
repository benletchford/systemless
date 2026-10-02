---
id: puzzler-103
kind: game
title: Puzzler 1.0.3
summary: Fit colorful polyomino pieces into each level's target shape.
developer: Bjorn Carlin
publisher: Bjorn Carlin
year: 1998
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
    systemless_version: 0.73.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged unregistered Info-Mac package opened in 68K Systemless. File >
      New Game and the reminder's OK button reached Level 1. Clicking a piece showed its
      placement shadow; a second click placed it in the target shape. The release
      browser fetched the same archive once and reproduced this move.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3899
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 3c18a8376a4b70beaf171d8290b350be7c4fe272914e8c6442746406ae4a54e2
    size_bytes: 169488
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=11456
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/puzzler-103.hqx
    rights_holder: Bjorn Carlin
    permission: >-
      The bundled Puzzler docs permit free copying and distribution without charge
      when the documentation and registration application are included. This is the
      complete unchanged original archive, including both files.
    notes: >-
      Original 169,488-byte Info-Mac BinHex/StuffIt package; SHA-256
      3c18a8376a4b70beaf171d8290b350be7c4fe272914e8c6442746406ae4a54e2.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2c04ec9cbdd658e43ff3514054b7f175c24e52b8b7e74e1b4655e19b61ef0e2f
    size_bytes: 5320
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3899
    permission: >-
      Fresh Systemless gameplay capture from the unchanged unregistered package.
      Underlying artwork remains its owner's property.
    notes: >-
      483-by-320 game-window crop at (9,31) from an 800-by-600 Systemless framebuffer
      after one piece was placed on Level 1.
references:
- https://info-mac.org/viewtopic.php?t=11456
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/puzzler-103.hqx
---

## Fit the pieces

![Puzzler Level 1 after a piece was placed](https://assets.systemless.org/catalogue/media/sha256/2c/2c04ec9cbdd658e43ff3514054b7f175c24e52b8b7e74e1b4655e19b61ef0e2f.png)

Choose **New Game** from **File** and dismiss the shareware reminder. Click
a piece, move its shadow into the target, and click again to place it. Use
the numeric keypad or I/J/K/L to rotate and mirror pieces. The unregistered
edition includes six levels.
