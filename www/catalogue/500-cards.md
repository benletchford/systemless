---
id: 500-cards
kind: game
title: 500 Cards 2.0.1
summary: >-
  Partner with the computer to bid, win tricks and race to 500 points in Denis
  Birnie's version of the classic partnership card game.
developer: Denis Birnie
year: 1998
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: b6047d79 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode and opened a game
      with the player and a computer partner against two computer players, with bidding
      under way. Clicking No Trumps placed a 6 No Trumps bid and enabled the Bid and
      Reset Bid buttons. A matched control without the click was identical until the
      input and then differed by 3,999 pixels. An exact replay matched both checkpoints,
      with zero exhausted frames. Heart pips draw in green. Trick play, scoring, network
      play and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5449
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: b6047d79 browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive. Scripted
      ordinary input clicked No Trumps during bidding. Twenty-second samples reached 59.95,
      59.80 and 60.26 guest ticks per second (host 60.99, 61.90 and 61.30 fps), with
      no console errors. This approves bounded browser launch; trick play and scoring
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5449
runtime:
  executable_path: 500 Cards 2.0.1/500 Cards v2.0.1
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: eaf058d5d9d3ed675bc8f5069e08587cfb13db25ef56b327ee95d6d2096b8bce
    size_bytes: 95455
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/crd/500-cards-201.hqx
    - https://github.com/benletchford/systemless/issues/5449
    license: Original shareware distribution notice
    rights_holder: Denis Birnie
    permission: >-
      The bundled Read Me says 500 Cards is shareware that may be freely distributed,
      provided it is not modified and its shareware status is made clear. Inclusion in
      a commercial package requires informing the author and obtaining consent.
      Preserve the complete unchanged archive.
    notes: The archive contains the 68K application, rules and Read Me.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 382ec9c23a6d677cfff58d3f0aa6b9092248f05013f9ec9a90e6f95db7672c46
    size_bytes: 8529
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5449
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: >-
      Native 68K table after placing a No Trumps bid, cropped to the game window
      contents.
references:
- https://github.com/benletchford/systemless/issues/5449
---

## Bid and take tricks

![500 Cards table](https://assets.systemless.org/catalogue/media/sha256/38/382ec9c23a6d677cfff58d3f0aa6b9092248f05013f9ec9a90e6f95db7672c46.png)

Team up with a computer partner against two computer players. Bid for trumps, take
tricks and be the first partnership to reach 500 points. The shareware version can
also play over a network with other players.

Choose a suit or **No Trumps** to bid, then click **Bid**. Click a card in your hand
to play it.
