---
id: where-in-time-is-carmen-sandiego-demo
kind: game
title: Where in Time is Carmen Sandiego? Demo
summary: >-
  Chase Carmen Sandiego's gang through history as an ACME time detective,
  following clues from era to era in Broderbund's educational adventure.
developer: Broderbund
publisher: Broderbund
year: 1990
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice and reached the ACME
      Detective Agency elevator. Pressing the elevator call button rode down to the
      lobby. A matched idle control was identical until the input and then differed by
      76,897 pixels. An exact replay matched both checkpoints, with zero exhausted frames.
      Time travel, case solving, and audio remain unverified. With 6 MHz pacing, three
      browser samples in the lobby passed at about 60 host fps and 60 guest ticks/s;
      default pacing reached only about 27 ticks/s.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5508
runtime:
  runtime_pacing:
    cpu_mhz: 6
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: ed90234382183fbd228315e8fb86d3e0375723529b19ff0bc6d02ae9bd6d4e3a
    size_bytes: 497793
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/where-in-time-is-carmen-sandiego
    - https://download.classicmacdemos.com/Carmen%20Time%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5508
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Broderbund
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: 68K black-and-white demo of Broderbund's time-travel detective game.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4f6ba01fea96fe639675c0b8889af2d0ca71a7ebd0544c1808aa323e30420b92
    size_bytes: 7198
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5508
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K ACME lobby after the elevator ride, cropped to the game window.
references:
- https://classicmacdemos.com/where-in-time-is-carmen-sandiego
- https://github.com/benletchford/systemless/issues/5508
---

## Time Crimes Division

![Where in Time is Carmen Sandiego? Demo](https://assets.systemless.org/catalogue/media/sha256/4f/4f6ba01fea96fe639675c0b8889af2d0ca71a7ebd0544c1808aa323e30420b92.png)

Report to the ACME Detective Agency, pick up your case and use the Chronoskimmer to track V.I.L.E. agents through history.

Click the elevator buttons and the objects around each room to explore.
