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
      empty data fork, nine CODE resources and no cfrg resource. Independent extraction
      parity and full unregistered-game coverage remain pending.
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

The character-selection portraits currently appear blank, though their locations
can be clicked. Browser gameplay, combat and saved games remain unverified.
