---
id: expert-blackjack
kind: game
title: Expert Blackjack 4.0
summary: >-
  Take a seat at a six-player casino blackjack table, practise strategy and run
  simulations in KT Enterprises' detailed blackjack trainer.
developer: KT Enterprises
year: 1999
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
    systemless_version: 67b0ef08 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. After OK on the
      demo welcome notice and the seat dialog, clicking Wager OK dealt cards to the
      player and five computer players and enabled Hit, Stay, Double and
      Surrender. A matched control without the click was identical until the input
      and then differed by 66,387 pixels. An exact replay matched both
      checkpoints, with zero exhausted frames. The demo omits fives and sevens from
      the shoe. Simulations, practice modes and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5447
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 67b0ef08 browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs
      of a locally served trunk build used the exact pinned original archive.
      Scripted ordinary clicks dismissed the welcome and seat dialogs and
      clicked Wager OK to deal. Twenty-second samples reached 60.00, 60.01 and
      60.00 guest ticks per second, with no console errors. This approves
      bounded browser launch; complete hands and simulations remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5447
runtime:
  executable_path: Expert Blackjack 4.0/Expert Blackjack 4.0
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/crd/expert-blackjack-40.hqx
    expected_sha256: 8349e867e9e7740fdc2931456a12638e49959b5f1cbe64ac1ef6fae5bf7b579a
    expected_size: 1384027
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/crd/expert-blackjack-40.hqx
    - https://github.com/benletchford/systemless/issues/5447
    license: Original unregistered distribution notice
    rights_holder: KT Enterprises
    permission: >-
      The bundled licence says Expert Blackjack may be distributed as long as all
      files are included without alteration, including on commercial CDs and
      servers. Distributing registered copies or registration codes is prohibited.
      Preserve the complete unchanged archive.
    notes: US$10 shareware demo; the unregistered shoe omits fives and sevens.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/expert-blackjack/table.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5447
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K table after the first deal, cropped to the game window contents.
references:
- https://github.com/benletchford/systemless/issues/5447
---

## Beat the dealer

![Expert Blackjack table](incoming/expert-blackjack/table.png)

Play casino blackjack against the dealer alongside five computer players with
different styles. Practise basic strategy and card counting, or run simulations
to test a system.

Click **OK** on the welcome notice and the seat dialog, then **Wager OK** to deal.
Use Hit, Stay, Double, Split and Surrender to play your hand.
