---
id: assassin
kind: game
title: Assassin 1.1.2
summary: >-
  Plot simultaneous moves and shots through a maze to hunt down rival assassins
  in Steven H. Taylor's turn-based networked strategy game.
developer: Steven H. Taylor
year: 1996
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
    systemless_version: 52de7794 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. Choosing Solo and
      accepting Game Setup opened the Dead Zone maze against two computer players.
      Clicking the on-screen Up and Right move buttons entered moves and moved the player's
      marker. A matched control without the clicks was identical until the input and
      then differed by 238 pixels. An exact replay matched every checkpoint, with zero
      exhausted frames. Keyboard move entry was not accepted in this run. Turn
      resolution, network play, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5425
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 78ec4f28 browser build with launch flag and 10 MHz pacing
    architecture: 68k
    environment: >-
      At the default 25 MHz budget, browser pacing held only 34.6 to 35.6 guest
      ticks per second. With runtime pacing at 10 MHz, three release-mode headless
      Chromium 156 (aarch64 Linux) worker/WebGL runs of a locally served trunk build
      used the actual Systemless-hosted archive, whose bytes matched the pinned size
      and SHA-256. Scripted ordinary clicks chose Solo, accepted Game Setup and
      entered moves with the on-screen buttons; the move list updated. Twenty-second
      samples reached 60.00, 60.00 and 60.01 guest ticks per second (host 61.04,
      61.04 and 60.01 fps), with no console errors. This approves bounded browser
      launch; turn resolution, network play and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5425
runtime:
  show_menu_bar: true
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 7ca8caa8df392db268823df910de0cf0732af77ea20d54681fc08b0b31a2377f
    size_bytes: 517649
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/assassin-112.hqx
    - https://github.com/benletchford/systemless/issues/5425
    license: Original free distribution notice
    rights_holder: Steven H. Taylor
    permission: >-
      The bundled Read Me permits copying and distributing the software freely, as
      long as that Read Me is included. Modified copies and charging for the software or
      its distribution are not permitted. Preserve the complete unchanged archive.
    notes: The archive contains the 68K application, instructions and Read Me.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 79875856f42337d23553f0635ccda792a3513579a7c070e5a4bc76620b43ea2a
    size_bytes: 19898
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5425
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K Dead Zone maze during move entry, cropped to the play field.
references:
- https://github.com/benletchford/systemless/issues/5425
---

## Plan your strike

![Assassin Dead Zone maze](https://assets.systemless.org/catalogue/media/sha256/79/79875856f42337d23553f0635ccda792a3513579a7c070e5a4bc76620b43ea2a.png)

Every player secretly plots a turn of moves and shots. All the moves then play
out at once. Hunt your target through the maze while avoiding the assassin hunting
you.

Choose **Solo** and click **OK**, then **OK** on Game Setup. Click the arrow, gun
and look buttons below the maze to enter moves, then click the cycle arrow to
play the turn.
