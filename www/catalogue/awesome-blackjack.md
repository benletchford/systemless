---
id: awesome-blackjack
kind: game
title: Awesome BlackJack 1.5
summary: >-
  Bet with colourful chips and play casino blackjack against the dealer in Bryan
  Arntson's photo-card blackjack table.
developer: Bryan C. Arntson
year: 1994
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
    systemless_version: f64ac896 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. After the title
      and the shoe shuffle, clicking the bet circle opened Place Your Bet; BET! and the L
      (deal) key dealt both hands and the dealer. A matched control without input was
      identical until the input and then differed by 100,755 pixels. An exact replay
      matched both checkpoints, with zero exhausted frames. The top-right table
      decoration draws over a white block. Hit, stand, payouts and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5451
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: f64ac896 browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive. Scripted
      ordinary input placed a bet and dealt with L. Twenty-second samples reached 60.01,
      61.19 and 58.63 guest ticks per second (host 62.11, 60.17 and 62.89 fps), with no
      console errors. This approves bounded browser launch; hit, stand and payouts
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5451
runtime:
  executable_path: Awesome BlackJack ƒ/Awesome BlackJack v1.5
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 302b6e21744d4c331546526c44c182f3050f93bf0d0358a8fef557a8f833c045
    size_bytes: 852283
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/crd/awesome-blackjack-15.hqx
    - https://github.com/benletchford/systemless/issues/5451
    license: Original free distribution notice
    rights_holder: Bryan C. Arntson
    permission: >-
      The bundled Read Me permits free distribution, as long as the documentation and
      program are distributed together and unaltered. Preserve the complete unchanged
      archive.
    notes: The archive contains the 68K application and two Read Me files.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0c13d9ad1122c3db05c83eac7d90a43436c2f6989f9c80756699435b5d747b05
    size_bytes: 41677
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5451
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K table after the first deal, cropped to the game window contents.
references:
- https://github.com/benletchford/systemless/issues/5451
---

## Hit or stand

![Awesome BlackJack table](https://assets.systemless.org/catalogue/media/sha256/0c/0c13d9ad1122c3db05c83eac7d90a43436c2f6989f9c80756699435b5d747b05.png)

Place your chips and play blackjack against a dealer who must draw to 16 and
stand on all 17s. Blackjack pays 3 to 2 and insurance pays 2 to 1.

Click the circle to place your bet and click **BET!**. Use the Action menu or its
keys to deal (L), hit (H), stand (S), double (D), split (P) or surrender (U).
