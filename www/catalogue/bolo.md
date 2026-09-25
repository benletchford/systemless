---
id: bolo
kind: game
title: Bolo
summary: >-
  Drive, fight and build across an island battlefield designed for shifting
  alliances between networked tank commanders.
developer: Stuart Cheshire
publisher: Stuart Cheshire
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: "0.61.1 + PR #2853"
    architecture: 68k
    environment: >-
      Deterministic local Tutorial replay from the unchanged original BinHex
      package. The network-selection dialog chooses Tutorial, the map draws
      blue water and green terrain, and holding Q advances the boat and the
      lesson sequence. Browser launch has not yet been approved.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2463
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/bolo/bolo-0997.hqx
    expected_sha256: c83deab0eefdde13d8868446530cc763536b9366b223da549b94b701f16af205
    expected_size: 835727
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=4778
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/bolo/bolo-0997.hqx
    rights_holder: Stuart Cheshire
    permission: >-
      The included ReadMe.Shareware permits unmodified electronic distribution when
      the complete original documentation accompanies the software; it prohibits
      commercial distribution and derivative works without permission.
    notes: >-
      Info-Mac identifies this as the author's official Bolo 0.99.7 package. The
      unchanged BinHex file is 835727 bytes with SHA-256
      c83deab0eefdde13d8868446530cc763536b9366b223da549b94b701f16af205.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/bolo/bolo-tutorial.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2462
    permission: >-
      Original gameplay screenshot captured for this catalogue from the
      unregistered Bolo shareware package. Underlying game artwork remains
      Stuart Cheshire's property.
    notes: >-
      Fresh deterministic Systemless 0.61.1 plus PR #2853 capture on
      2026-09-25, after the boat moves in the local Tutorial. Cropped the
      800-by-600 guest framebuffer to the 472-by-246 game content surface,
      excluding desktop, menu bar and window chrome without altering game
      pixels. PNG SHA-256
      3d4681856b7d859536c7b2f0371642a134e568cb2028eb0fc733cd0526070868;
      10,156 bytes.
references:
- https://info-mac.org/viewtopic.php?t=4778
---

## The official 0.99.7 package

This is Stuart Cheshire's complete electronic distribution: Bolo itself, its
sound file, the Standard Autopilot brain and the manuals and technical notes that
travelled with the game. It remains in the original BinHex wrapper submitted to
Info-Mac. The bundled shareware notice permits free, noncommercial electronic
redistribution of this complete, unmodified package; it asks players to pay
the shareware fee after a one-month evaluation. No registered copy or key is
included.

## Tank country

![Bolo's island Tutorial after moving the boat](incoming/bolo/bolo-tutorial.png)

Bolo's islands are working landscapes rather than fixed arenas. A tank can cut
through forest, lay roads, repair bridges, place mines and move pillboxes while
trying to capture refuelling bases. Limited sight makes every patch of trees a
possible ambush, and the alliance system leaves room for cooperation, betrayal
and long territorial campaigns.

The included tutorial teaches the terrain and construction tools without a
network. The larger game was made for groups: up to sixteen commanders sharing a
map, with optional programmable “brains” assisting their tanks.

Systemless currently reaches the local Tutorial, paints its water and terrain,
and responds to the boat's forward control. Browser launch remains disabled
until the released runtime and hosted assets pass an in-browser check.
