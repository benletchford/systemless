---
id: utopia
kind: game
title: Utopia 1.1.0
summary: Find the one different color before the timer runs out.
developer: Gilles Esposito-Farese and Nicolas Graner
publisher: Gilles Esposito-Farese and Nicolas Graner
year: 1999
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
    systemless_version: 0.72.0 + deterministic runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Utopia 1.1.0 archive launched in 68K Systemless. After continuing
      past the initial color-display warning, Space started a 3-by-3 color round.
      Clicking the unique center tile advanced to the next board and displayed a nonzero
      Last board and Average score. Selecting Huge Squares enlarged the board without
      changing its rules. The release browser build also reached the color board from the
      same archive, and clicking the unique tile advanced to the next board with a
      nonzero score. With 10 MHz pacing, a three-second browser sample measured about 60
      host frames and 45 guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3793
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 2360801c5d9913324a5222c6a417e6d4ef455188961af6b60d82d09a756ae79b
    size_bytes: 128238
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/utopia-11.hqx
    rights_holder: Gilles Esposito-Farese and Nicolas Graner
    permission: >-
      The bundled Utopia110.readme permits free copying and distribution of the
      complete package for non-commercial use. This entry preserves the unchanged archive
      and offers it without charge.
    notes: >-
      Original 128,238-byte BinHex/StuffIt package, SHA-256
      2360801c5d9913324a5222c6a417e6d4ef455188961af6b60d82d09a756ae79b.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 843b69732a6c755b8dc51de142735cb0ee66ac08147d1ddc7d7761f9dc9a6745
    size_bytes: 11340
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3793
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owners' property.
    notes: >-
      Full 800-by-600 Systemless framebuffer showing a scored 3-by-3 round with Huge
      Squares selected.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/utopia-11.hqx
---

## Find the unique color

![Utopia after a scored color board](https://assets.systemless.org/catalogue/media/sha256/84/843b69732a6c755b8dc51de142735cb0ee66ac08147d1ddc7d7761f9dc9a6745.png)

If the color-display warning appears, choose **Continue**. Press **Space** or
choose **File > New** to start. Click the square whose color appears only once.
A correct choice advances the board and raises your score. The **Options**
menu lets you enlarge the squares or change the time limit; a round ends after
ten boards.

The complete Utopia 1.1.0 package is freeware for non-commercial distribution.
