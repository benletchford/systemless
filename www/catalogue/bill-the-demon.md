---
id: bill-the-demon
kind: game
title: Bill the Demon
summary: Walk, jump and scream through the nine circles of Hell as a hungry little demon.
developer: James Burton / John Burton / Jeffrey Loff
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: c796d054 native; runtime code identical through merged 99cc7439
    architecture: 68k
    environment: >-
      Complete unchanged original colour package and exact billdemon.cpt/Bill the Demon
      executable, explicit 68K, default 25 MHz/800x600/depth 8 without partition override.
      Click dismisses the original title; Command-N starts Circle 1. Holding D for 20
      frontend ticks moves right; W for 20 ticks jumps and scrolls the scenery. The
      equal-clock omitted-key control stays at the starting platform. Both fresh repeats
      pass six measured causal pixels and six exact prior screenshots at frontend/guest
      tick 540/1140, with zero exhausted frames. Actual qualified images inspected;
      own-run references establish consistency, not hardware accuracy. Native binary
      SHA-256 7ca0f7195dc8910fe687bf0d470cf45a7e2de94a60a528a964fc5b7600867e03.
      Space was exercised but its effect is not yet visually established. Full levels,
      souls/scoring, persistence, audio fidelity, browser pacing and released public play
      remain unverified. Launch stays disabled pending browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5831
runtime:
  executable_path: billdemon.cpt/Bill the Demon
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mac/info-mac/game/arc/bill-the-demon.hqx
    expected_sha256: 7f10d5f30949cd60f9a6bdda5cecfea8b55454fc373dd19040bf97e7cf6d07d2
    expected_size: 1033479
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/bill-the-demon.hqx
    - https://github.com/benletchford/systemless/issues/5831
    license: Original unchanged free-distribution grant; no commercial sale
    rights_holder: James Burton; retained original contributors
    permission: >-
      The complete original Instructions.txt explicitly permits distribution anywhere
      and however desired provided the game remains free, is not sold commercially and
      is not altered. Missing instructions count as alteration. Preserve both original
      files and all forks, resources, credits and notices. No game modifications.
    notes: >-
      Full 13721-byte instructions reviewed. Original title and instructions identify
      copyright 1996 James Burton, with John Burton graphics and Jeffrey Loff music.
      Independent extraction matches both paths, all four forks and both Finder records;
      the production pack retains the original billdemon.cpt wrapper. Strict KPK end
      1527299, SHA-256 fff37d6ebab7a5ccc56e849d4d955fceb21e3c1d32abea08496bdb6717f98c5f.
      The black-and-white variant is not a separate game count.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/bill-the-demon/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5831
    permission: Original gameplay capture of the unchanged freely distributable game.
    notes: >-
      Circle 1 after movement and jump. Native capture cropped to (149,133)-(663,504),
      514x371, without resampling; excludes native window framing, menu and host interface.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/bill-the-demon.hqx
- https://github.com/benletchford/systemless/issues/5831
---

## A little demon with a loud voice

![Bill jumps above the starting platform](incoming/bill-the-demon/gameplay.png)

Click the title to dismiss it, then choose Game → New Game. Use A/D or Left/Right
to walk, W or Up to jump, and Space or Return to scream or eat. Screaming consumes
hunger; eating souls replenishes it. The complete original instructions accompany
the game download.
