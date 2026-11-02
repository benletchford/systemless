---
id: quest-of-yipe-ii
kind: game
title: Quest of Yipe! II
summary: Explore the lands around Yipe, fight monsters and gather supplies for the town.
developer: Kevin Kinell / Yipe Software
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-02"
    tester: Catalogue maintainer
    systemless_version: fd35169ca
    architecture: 68k
    environment: >-
      Original author-hosted version 1.05, 800-by-600 at 8-bit depth, explicit 68K
      preference. New Game, Skip Intro and the first character selection reach the town.
      D moves the avatar one tile right; a same-duration no-key control remains in
      place, with 922 differing pixels in the screenshots. The selection portraits are
      blank, tracked in issue 5529, although their hit regions work. No browser pacing,
      combat, saves or audio verification is claimed; launch remains disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5530
  - date: "2026-11-03"
    tester: Catalogue maintainer
    systemless_version: 56f634acd
    architecture: 68k
    environment: >-
      Original version 1.05, 800-by-600/8-bit, with PlotCIcon snapshot
      repair in PR 5537. Native replay shows the original character choices
      and passes 3 portrait pixel assertions with zero exhausted frames;
      selecting a character and moving right reaches the same gameplay image
      as the pre-repair replay. Chrome 156 worker/WebGL preview uses local
      frontend bytes and the untouched promoted archive (HTTP 200), selects a
      visible portrait and moves right. A 20-second sample after input,
      including idle town time, reports 59.018 guest ticks/sec with no console
      errors. Public release, combat, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5537
runtime:
  executable_path: Quest of Yipe! II/Yipe! II  v1.05
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: db8c6359c2446a508077f7262a85a1df1f0fa2bf24c2ff8e8d3890b25055011d
    size_bytes: 330880
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.yipesoftware.com/yipeii.htm
    - https://github.com/benletchford/systemless/issues/5530
    license: Original unchanged-package distribution permission
    rights_holder: Kevin Kinell / Yipe Software
    permission: >-
      The bundled Yipe! License permits electronic and other distribution of the
      complete software without modification; it prohibits distribution of registered
      versions. This is the author-hosted shareware package, kept complete and unchanged,
      including the registration form and licence.
    notes: >-
      Production packing identifies four files including the original application,
      licence, registration form and folder icon. The application has an empty data fork,
      eight CODE resources and no cfrg resource. Independent unar extraction matches
      all four files, eight forks and Finder metadata. Full unregistered-game
      coverage remains pending.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 561544ac7876d6db1cef8720331d0c178350686bcadea95b71594939ef63b9a9
    size_bytes: 54308
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5530
    permission: Systemless gameplay capture requested by the maintainer.
    notes: Unscaled crop of the original game town after moving right.
references:
- https://www.yipesoftware.com/yipeii.htm
---

![The player in the town of Yipe](https://assets.systemless.org/catalogue/media/sha256/56/561544ac7876d6db1cef8720331d0c178350686bcadea95b71594939ef63b9a9.png)

Create a character and explore the town and surrounding lands. Use the keyboard
to move and encounter monsters, shops and treasures. This is the original
shareware edition; its registration limits remain in place.

Character selection and movement were checked in the repaired browser preview.
Combat, saved games and the public release remain unverified.
