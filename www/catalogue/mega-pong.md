---
id: mega-pong
kind: game
title: MegaPong World Tournament
summary: >-
  Battle the computer or a friend in JFX Studios' colourful, sound-packed take on
  Pong.
developer: Jean-François Nadeau and Félix Lapointe (JFX Studios)
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 44af7d48 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. File > New Game
      started a one-player match against the computer. Holding Q (player 1 up) for 40
      ticks raised the left paddle. A matched control without the key was identical until
      the input and then differed by 4,037 pixels. An exact replay matched both
      checkpoints, with zero exhausted frames. A small leftover artefact appears above the
      court's top-right corner. Full matches and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5432
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 44af7d48 browser build with launch flag and 10 MHz pacing
    architecture: 68k
    environment: >-
      At the default 25 MHz budget, browser pacing reached only 31.7 to 36.6 guest
      ticks per second. With runtime pacing at 10 MHz, three release-mode headless
      Chromium 156 (aarch64 Linux) worker/WebGL runs of a locally served trunk build used the
      exact pinned original archive. Scripted ordinary input started a game with
      Command-N and held Q; live play was visible, with the computer scoring. Twenty-second
      samples reached 59.73, 62.17 and 60.75 guest ticks per second (host 62.93, 63.24
      and 59.74 fps), with no console errors. This approves bounded browser launch;
      full matches and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5432
runtime:
  executable_path: MegaPong ƒ/MegaPong
  runtime_pacing:
    cpu_mhz: 10
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 123a773e69782a77fd9798096fd3d6fd398cab510af0df30085a4493b23abb51
    size_bytes: 790530
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/mega-pong.hqx
    - https://github.com/benletchford/systemless/issues/5432
    license: Original noncommercial distribution notice
    rights_holder: JFX Studios
    permission: >-
      The bundled ReadMe permits free distribution for non-profit purposes. CD-ROM or
      other for-profit distribution requires the authors' prior written consent.
      Preserve the complete unchanged archive.
    notes: The archive contains the 68K application and ReadMe.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: cdcda4142d5e73030ac603f89438f1d7c3e86a2fbb6bc3593c12adfffb8b7018
    size_bytes: 83271
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5432
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: >-
      Native 68K one-player match after the left paddle moves up, cropped to the
      court.
references:
- https://github.com/benletchford/systemless/issues/5432
---

## Return the ball

![MegaPong one-player match](https://assets.systemless.org/catalogue/media/sha256/cd/cdcda4142d5e73030ac603f89438f1d7c3e86a2fbb6bc3593c12adfffb8b7018.png)

Bat the ball past your opponent in this lively Pong tournament, with three
difficulty levels and a two-player mode.

Choose **File > New Game**. Player 1 uses Q and A to move up and down; player 2
uses O and L. Change players, difficulty, points per match and keys in the
Options menu. Press Escape to abort a game.
