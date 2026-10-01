---
id: whack-it-weenie-edition
kind: game
title: WhackIt! Weenie Edition 1.0
summary: Whack the moles as they pop out of nine holes and build a high score.
developer: Matthew Johnson
publisher: Matt the Great Software Enterprises
year: 1997
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
    systemless_version: 0.72.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged author-submitted Info-Mac Weenie Edition launched in 68K
      Systemless. In a deterministic replay, dismissing the introductory dialog and clicking
      the title window started a live nine-hole board. A mouse click struck a visible
      mole, increased the score from 0 to 50, and a new mole appeared. The release browser
      build fetched the archive once, reached the live board, showed moving moles and
      the game-over and high-score screens, and ran a five-second sample at about 60
      host frames and 60 guest ticks per second. A browser hit was not confirmed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3777
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 45bf0b317792530c78a04092ed7c7c3f2e16623724c0fee405c695345a3b718f
    size_bytes: 303795
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/whack-it.hqx
    rights_holder: Matt the Great Software Enterprises
    permission: >-
      The bundled Read Me calls the Weenie Edition free and expressly permits giving
      copies to friends or uploading it to a website or online service. It
      distinguishes the paid Normal and Extra Groovy editions. This entry preserves the complete
      unchanged Weenie Edition package and Read Me.
    notes: >-
      Original 303,795-byte BinHex/StuffIt archive, SHA-256
      45bf0b317792530c78a04092ed7c7c3f2e16623724c0fee405c695345a3b718f.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8f6a4def316b6cfc7434327ce6b4f558f5ac2305a34753d80ce71710a355ec68
    size_bytes: 5396
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3777
    permission: >-
      Fresh Systemless gameplay capture from the unchanged free edition. Underlying
      artwork remains its owner's property.
    notes: >-
      Exact 514-by-387 game-content crop at (143,111) from an 800-by-600 Systemless
      framebuffer just after a successful hit; score 50 and another mole visible.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/whack-it.hqx
---

## Whack the moles

![WhackIt! board after a successful hit](https://assets.systemless.org/catalogue/media/sha256/8f/8f6a4def316b6cfc7434327ce6b4f558f5ac2305a34753d80ce71710a355ec68.png)

Dismiss the opening message, then click the title window or choose **New Game**
from the **File** menu. Click a mole when it appears to score points before
it disappears into its hole.
