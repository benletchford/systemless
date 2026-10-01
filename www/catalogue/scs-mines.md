---
id: scs-mines
kind: game
title: SCS Mines 1.0
summary: Reveal safe squares and mark the hidden mines.
developer: Silicon Creek Software
publisher: Silicon Creek Software
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged author-submitted FAT archive launched in 68K Systemless. Clicking
      a covered square opened neighboring cells and displayed numbered mine clues. The
      release browser build fetched the same archive once, opened the 68K game, and
      the same click revealed numbered cells.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3808
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: bcb8d3a1ac19cabc888a9053b49076ba9b3b445242aa85baadded492ae7a0042
    size_bytes: 168544
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/scs-mines-10.hqx
    rights_holder: Silicon Creek Software
    permission: >-
      The bundled About file calls SCS Mines freeware. It allows CD-ROM distribution
      while asking distributors to contact the author first as a courtesy. This entry
      offers the complete, unchanged author-submitted archive online without charge.
    notes: >-
      Original 168,544-byte BinHex/StuffIt archive, SHA-256
      bcb8d3a1ac19cabc888a9053b49076ba9b3b445242aa85baadded492ae7a0042.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 9c77424d9fd2e72453690bfb6a2db0933b2ede9118daf6fae60e331b19b2cd2f
    size_bytes: 2947
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3808
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only crop from the release browser build showing revealed cells and
      numbered clues.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/scs-mines-10.hqx
---

## Clear the field

![SCS Mines after revealing a square](https://assets.systemless.org/catalogue/media/sha256/9c/9c77424d9fd2e72453690bfb6a2db0933b2ede9118daf6fae60e331b19b2cd2f.png)

Click a covered square to reveal it. Numbers show how many mines touch that
square; use the clues to find the remaining safe squares. This is the original
freeware FAT release, with both 68K and PowerPC builds.
