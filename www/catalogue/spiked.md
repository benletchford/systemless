---
id: spiked
kind: game
title: Spiked 2.1
summary: >-
  Fly, thrust and shoot around a spiked mine in Chuck Groom's frantic two-player
  arena game.
developer: Chuck Groom
year: 1997
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
    systemless_version: 9e16c2cf deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. It reached the
      title menu, and New Game opened the two-player arena with items and asteroids.
      Holding the green player's W thrust key for 90 ticks flew the green ship across the
      arena. A matched control without the key was identical until the input and then
      differed by 826 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. Combat, items, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5421
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 9e16c2cf browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive. Scripted
      ordinary input chose New Game, reached the live two-player arena and held W.
      Twenty-second samples reached 61.28, 57.17 and 61.70 guest ticks per second (host 60.26,
      58.16 and 60.67 fps), with no console errors. This approves bounded browser
      launch; browser movement was not measured separately, and deployed replay, full
      matches and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5421
runtime:
  executable_path: Spiked 2.1/Spiked 2.1
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: c94bf9dac115a4f91bb66f6453522a8f9db756aa194a7cd13ac3c21e4215be24
    size_bytes: 1312161
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/spiked-21.hqx
    - https://github.com/benletchford/systemless/issues/5421
    license: Original freeware distribution notice
    rights_holder: Charles Groom
    permission: >-
      The bundled About Spiked document says Spiked 2.1 is freeware that may be
      copied and distributed freely, as long as that document is included. Modified copies
      and charging for the software or its distribution are not permitted. Preserve the
      complete unchanged archive.
    notes: >-
      The archive contains the 68K application, About Spiked, physics, data and
      preferences files.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0ba6270d8094e9b28c7295a246770f901f63f0fb3ffd34330cea5e984b73b235
    size_bytes: 86567
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5421
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: >-
      Native 68K arena after the green ship thrusts forward, cropped to the play
      field.
references:
- https://github.com/benletchford/systemless/issues/5421
---

## Battle in the arena

![Spiked two-player arena](https://assets.systemless.org/catalogue/media/sha256/0b/0ba6270d8094e9b28c7295a246770f901f63f0fb3ffd34330cea5e984b73b235.png)

Two ships share an arena with a deadly spiked mine, asteroids and power-up
gifts. Thrust, turn and use your items to destroy your opponent.

Choose **New Game**. The green player uses A and D to turn, W to thrust, S to use
an item and Q to change items. The red player's keys can be set on the Prefs
screen.
