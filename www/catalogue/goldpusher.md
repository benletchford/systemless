---
id: goldpusher
kind: game
title: GoldPusher 1.4
summary: Guide a gold pot to its rainbow through rooms of hazards and puzzles.
developer: Erich Friedman, Daniel Vree and Willem Vree
publisher: Willem Vree
year: 1996
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
    systemless_version: 0.72.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged author-submitted Info-Mac archive launched in 68K Systemless. A
      key press left demo mode and opened Room 1, Look around! In matched deterministic
      runs, holding the documented j key moved the player down from the upper platform
      while the no-input run left the player in place. The release browser build
      fetched the archive once, opened Room 1, and moved the player down with the same key. A
      five-second active-room sample ran at about 60 host frames and 60 guest ticks
      per second while room hazards continued to animate.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3788
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 9824a42a3de896da2bdaf34a8923212562b91500ac7ab1761086c1c68b8b68c2
    size_bytes: 816230
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gold-pusher-14.hqx
    rights_holder: Erich Friedman, Daniel Vree and Willem Vree
    permission: >-
      The original Info-Mac abstract, submitted from Willem Vree's address, calls
      GoldPusher freeware while identifying its registration limits. The bundled guide
      describes the unregistered game as largely freeware, with rooms above 100 and
      solutions above room 20 requiring registration. This entry preserves the complete,
      unchanged author-submitted package.
    notes: >-
      Original 816,230-byte BinHex/StuffIt archive, SHA-256
      9824a42a3de896da2bdaf34a8923212562b91500ac7ab1761086c1c68b8b68c2.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: e013b9a0244d0435b9cead78d6586c500ca8bcff97eee74f48429ac787f8324e
    size_bytes: 22698
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3788
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owners' property.
    notes: >-
      Exact 514-by-365 game-content crop at (143,119) from an 800-by-600 Systemless
      framebuffer in Room 1 after the player moved down.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/gold-pusher-14.hqx
---

## Push the pot

![GoldPusher Room 1 after moving down](https://assets.systemless.org/catalogue/media/sha256/e0/e013b9a0244d0435b9cead78d6586c500ca8bcff97eee74f48429ac787f8324e.png)

Press a key or click to leave demo mode. Move with **h**, **j**, **k**, and
**l** (left, down, up, and right), or use the numeric keypad. Guide the
gold pot onto the rainbow to complete a room. The unregistered edition
includes the first 100 rooms; later rooms and some solution playback require
a registration code.
