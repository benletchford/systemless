---
id: heartquest
kind: game
title: HeartQuest
summary: Steer a butterfly through the forest to collect hearts and avoid flypaper.
developer: Ingemar Ragnemalm
year: 1997
architectures:
- 68k
- ppc
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
      MacBinary/Compact Pro package. Exact nested executable, explicit 68K slice, 800-by-600
      display and 8-bit depth. Game > New game and an ordinary click start level 1; mouse
      steering moves the butterfly, collects a heart and raises score from 0 to 10.
      Inspected captures and a fresh six-assertion replay pass at 929 frontend / 1504
      guest ticks with zero exhausted frames. Both original files and both forks
      independently match unar extraction byte-for-byte. Longer ordinary mouse inputs
      also reverse vertical motion; short inertia-heavy runs alone do not establish
      a control defect. Complete levels, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4452
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 1451faa8e2beb7b79e12933603218ecdb7510d83
    architecture: 68k
    environment: >-
      Ordinary Chrome worker/WebGL preview v0.94.0, 800-by-600/8-bit display and
      crossOriginIsolated true. Game > Macho mode is turned off for original normal
      mode, then New game and an ordinary click start level 1. Longer bounded mouse
      input moves the butterfly down, left, up and right and collects a heart;
      score rises from 0 to 10. Actual captures inspected. Local-origin CORS uses
      only an unchanged integrity-checked hosted archive fixture matching the
      original SHA-256 and 213248-byte size. Public replay, complete levels,
      saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4452
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 0f4ee1ae9fa59ab6d6be0c511aee0c5d920e4142
    architecture: ppc
    environment: >-
      Actual native v0.93.0 PowerPC replay of the unchanged original fat package,
      explicit PPC slice and exact nested executable, 800-by-600 display and 8-bit
      depth. Original normal-mode selection, New game and click start level 1;
      longer ordinary mouse inputs move the butterfly across the playfield and
      collect hearts, raising score from 0 through 10 to 30. Actual captures
      inspected. Fresh 16-pixel assertion replay passes at 1476 frontend / 1429
      guest ticks with zero exhausted frames. OpenRFPerm #4449 and rounded erase
      #4450 supply the required runtime support and are now merged. PPC browser
      qualification, public replay, complete levels, saves and audio remain pending.
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
    type: sha256
    sha256: 54b2717013f19de20f10c7cc74a005f4f6b3417934f3c14d0e6101fbe4b87fc3
    size_bytes: 213248
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
      The original HeartQuest 1.1 docs explicitly describe the game as freeware and
      permit giving it to friends, uploading it to BBSs and inclusion on CD-ROM
      compilations. The complete unchanged original-author distribution retains its notices and
      documentation. All rights remain with the original authors.
    notes: >-
      Original 213248-byte MacBinary/Compact Pro package, not repacked or converted.
      The game has four nonzero CODE segments plus CODE 0 and a native pwpc cfrg/PEF
      slice. The original docs identify the Valentine 1997 version 1.1 release. This
      native 68K and PPC gameplay are qualified; the 68K browser route is qualified and
      the PPC browser route remains pending. Launch stays disabled until both browser
      routes are checked.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 76916e398a0ee2aeec52ac1ad5d7b570727fb9aafeb74134eac0b0614e6b1226
    size_bytes: 64537
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4452
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Lossless 800-by-580 crop of the inspected original 800-by-600 native guest
      frame, removing only the top 20-pixel Classic menu bar. Shows the butterfly, original
      forest/hearts and score 10 after collection, with no host UI or redrawing.
references:
- https://www.lysator.liu.se/~ingemar/games.html
---

![HeartQuest butterfly collecting hearts](https://assets.systemless.org/catalogue/media/sha256/76/76916e398a0ee2aeec52ac1ad5d7b570727fb9aafeb74134eac0b0614e6b1226.png)

Choose **New game** from **Game**, then click to start a level. Move the mouse to
steer the butterfly, collect the floating hearts and avoid the flypaper. The
original game includes normal levels, bonus levels and a harder mode.

This is the complete original **HeartQuest 1.1** freeware distribution, including
its documentation and copyright notices. The original package includes both 68K and PowerPC versions.

Bounded native testing verifies starting play, steering and heart collection on
both original versions. Ordinary 68K browser testing also verifies movement and
score advancement in normal mode. PowerPC browser qualification and launch
approval remain pending. Public gameplay, complete levels, saves and audio remain
unverified.
