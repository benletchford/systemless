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
  - date: "2026-11-06"
    tester: Catalogue maintainer
    systemless_version: "0.110.1 (published release, 31cb5665fabc)"
    architecture: 68k
    environment: >-
      The original Info-Mac archive and its three files/six forks were rechecked
      against independent extraction. Native input selected and submitted 7 Spades,
      finished the kitty phase, and played King of Spades. Three computer replies
      completed a trick and the hand fell from ten cards to nine; the matched
      no-card-click control retained all ten. Exact native reference captures and
      eight measured gameplay pixels passed with zero exhausted frames. Direct
      systemless.org trusted mouse input reproduced the trick, while its control
      stayed ready to play. Quiet 20-second post-trick/control board samples passed
      unchanged pacing gates at 59.320/60.522 guest ticks per second, with maximum
      frames of 26.1/23.5 ms and no captured console errors. Both actual browser
      archive responses matched the original pin. Full hands, scoring, network
      play, persistence and audio fidelity remain unverified; heart pips remain green.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5649
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
      a commercial package requires informing the author and either obtaining consent
      or providing a free copy of the full package.
      Preserve the complete unchanged archive.
    notes: >-
      The archive contains the 68K application, rules and Read Me. Fresh independent
      extraction on 2026-11-06 matched all three paths, six forks and each file's
      Finder metadata against the production packer, including strict end of pack.
      The application has an empty data fork and a 132,691-byte resource fork
      (SHA-256 dd8a5411e8cb41d5b16611aeee6eda065732fc938ca2228adf366bb54280ee34),
      with two CODE resources and no cfrg. Read Me! is 10,980 bytes, SHA-256
      227a69914d2044ee6bedafa61cd15e991bd2175818182b92d564af484cecc5e9.
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

A released-site check submitted a **7 Spades** bid, finished the kitty phase,
and played **King of Spades**. The computer players completed the trick and the
player's hand dropped from ten cards to nine. A matched control without the
card click stayed ready to play. This verifies a bounded trick; full hands,
scoring, network play, save persistence and audio fidelity remain unverified.
Heart pips currently appear green.
