---
id: yipe-iii
kind: game
title: Yipe! III
summary: Search for the king's stolen stamps and missing daughters in a whimsical RPG.
developer: Kevin Kinell / Yipe Software
year: 1998
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
      Original author-hosted version 1.4, 800-by-600 at 8-bit depth, explicit 68K
      preference. New Game, Done, first character choice and Skip Intro reach the town. A
      moves the avatar one tile left; a same-duration no-key control remains in place,
      with 1054 differing pixels in the screenshots. Character selection portraits are
      blank, tracked in issue 5529, although their hit regions work. No browser pacing,
      combat, saves or audio verification is claimed; launch remains disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5530
  - date: "2026-11-03"
    tester: Catalogue maintainer
    systemless_version: 56f634acd
    architecture: 68k
    environment: >-
      Original version 1.4, 800-by-600/8-bit, with PlotCIcon snapshot
      repair in PR 5537. Native replay shows the original character choices
      and passes 4 portrait pixel assertions with zero exhausted frames;
      selecting a character and moving left reaches the same gameplay image
      as the pre-repair replay. Chrome 156 worker/WebGL preview uses local
      frontend bytes and the untouched promoted archive (HTTP 200), selects a
      visible portrait and moves left. A 20-second sample after input,
      including idle town time, reports 58.953 guest ticks/sec. A matched
      no-key control reports 59.865 ticks/sec and retains the starting position;
      its capture differs from the input run by 903 pixels. Neither run
      reports console errors. Public release, combat, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5537
runtime:
  executable_path: Yipe! III ƒ/Yipe! III  v1.4
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: e0638f6c10dcb4f430771fcba80afb7a3a1ab89b84ae4834658a073ec64e9ccd
    size_bytes: 621950
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.yipesoftware.com/yipeiii_MAC.html
    - https://github.com/benletchford/systemless/issues/5530
    license: Original unchanged-package distribution permission
    rights_holder: Kevin Kinell / Yipe Software
    permission: >-
      The bundled Yipe! License permits electronic and other distribution of the
      complete software without modification; it prohibits distribution of registered
      versions. This is the author-hosted shareware package, kept complete and unchanged,
      including Monsters, registration form and licence.
    notes: >-
      Production packing identifies five files including the original application,
      Monsters data, licence, registration form and folder icon. The application has an
      empty data fork, nine CODE resources and no cfrg resource. Independent unar
      extraction matches all five files, ten forks and Finder metadata. Full
      unregistered-game coverage remains pending.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: dfea1085d6bb8f1e216cec6787da3a0b5f8e1f3fdef60472cffc2c08621efabf
    size_bytes: 99831
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5530
    permission: Systemless gameplay capture requested by the maintainer.
    notes: Unscaled crop of the original game town after moving left.
references:
- https://www.yipesoftware.com/yipeiii_MAC.html
---

![The player exploring the town of Yipe](https://assets.systemless.org/catalogue/media/sha256/df/dfea1085d6bb8f1e216cec6787da3a0b5f8e1f3fdef60472cffc2c08621efabf.png)

Create a character and search Yipe's world for the king's missing stamps and
princesses. The original unregistered shareware edition limits access to later
caves; no registration bypass is included.

Character selection and movement were checked in the repaired browser preview.
Combat, saved games and the public release remain unverified.
