---
id: heartquest
kind: game
title: HeartQuest
summary: Steer a butterfly through the forest to collect hearts and avoid flypaper.
developer: Ingemar Ragnemalm
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 0f4ee1ae9fa59ab6d6be0c511aee0c5d920e4142
    architecture: 68k
    environment: >-
      Bounded native v0.93.0 replay of the unchanged original-author 1.1 fat
      MacBinary/Compact Pro package. Exact nested executable, explicit 68K slice,
      800-by-600 display and 8-bit depth. Game > New game and an ordinary click
      start level 1; mouse steering moves the butterfly, collects a heart and raises
      score from 0 to 10. Inspected captures and a fresh six-assertion replay pass
      at 929 frontend / 1504 guest ticks with zero exhausted frames. Both original
      files and both forks independently match unar extraction byte-for-byte.
      PPC startup and movement work with compatibility fixes, but sustained steering
      and heart collection remain under investigation; PPC is not launch-approved.
      Browser/public gameplay, complete levels, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4452
runtime:
  executable_path: HeartQuest-fat-11.cpt/HeartQuest (fat) 1.1
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: url
    url: https://ftp.lysator.liu.se/pub/mac/games/HeartQuest-fat-11.cpt.bin
    expected_sha256: 54b2717013f19de20f10c7cc74a005f4f6b3417934f3c14d0e6101fbe4b87fc3
    expected_size: 213248
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.lysator.liu.se/pub/mac/games/HeartQuest-fat-11.cpt.bin
    - https://www.lysator.liu.se/~ingemar/games.html
    - https://github.com/benletchford/systemless/issues/4452
    license: Original freeware distribution grant
    rights_holder: Ingemar Ragnemalm; graphics by Susanne and Ingemar Ragnemalm
    permission: >-
      The original HeartQuest 1.1 docs explicitly describe the game as freeware
      and permit giving it to friends, uploading it to BBSs and inclusion on CD-ROM
      compilations. The complete unchanged original-author distribution retains
      its notices and documentation. All rights remain with the original authors.
    notes: >-
      Original 213248-byte MacBinary/Compact Pro package, not repacked or converted.
      The game has four nonzero CODE segments plus CODE 0 and a native pwpc cfrg/PEF
      slice. The original docs identify the Valentine 1997 version 1.1 release.
      This entry currently qualifies only its 68K route; PPC work continues in
      issues 4447 and 4448 and the catalogue delivery issue.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/heartquest/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4452
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Lossless 800-by-580 crop of the inspected original 800-by-600 native guest
      frame, removing only the top 20-pixel Classic menu bar. Shows the butterfly,
      original forest/hearts and score 10 after collection, with no host UI or redrawing.
references:
- https://www.lysator.liu.se/~ingemar/games.html
---

![HeartQuest butterfly collecting hearts](incoming/heartquest/gameplay.png)

Choose **New game** from **Game**, then click to start a level. Move the mouse to
steer the butterfly, collect the floating hearts and avoid the flypaper. The
original game includes normal levels, bonus levels and a harder mode.

This is the complete original **HeartQuest 1.1** freeware distribution, including
its documentation and copyright notices. This entry uses its verified 68K version;
the package also includes a PowerPC version whose support is still being tested.

Bounded native testing verifies starting play, steering and collecting a heart
with score advancement. Browser and public gameplay, full levels, saves and audio
remain unverified.
