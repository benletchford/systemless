---
id: angband
kind: game
title: Angband
summary: Explore a deep dungeon in the classic fantasy roguelike.
developer: Angband contributors
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
architecture_archives:
  68k: classic
  ppc: powerpc
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 238fd95fd419c3f6a59940b43a208268bfa80d03
    architecture: 68k
    environment: >-
      Bounded native replay of this architecture's separate intact original port.
      Ordinary character creation, initial-town staircase descent, message acknowledgement
      and west/south movement inside a generated dungeon at 50 feet. Actual captures
      inspected. Repeat passes four measured movement and cleared-cell pixel assertions
      at 3016 frontend ticks with zero budget exhaustion. Town movement also checked.
      Combat, full levels, sustained play, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4405
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 238fd95fd419c3f6a59940b43a208268bfa80d03
    architecture: ppc
    environment: >-
      Bounded native replay of this architecture's separate intact original port.
      Ordinary character creation, initial-town staircase descent, message acknowledgement
      and west/south movement inside a generated dungeon at 50 feet. Actual captures
      inspected. Repeat passes four measured movement and cleared-cell pixel assertions
      at 2958 frontend ticks with zero budget exhaustion. Town movement also checked.
      Combat, full levels, sustained play, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4405
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 4dc3f680e66aaa57769c086c348dd922193914a0
    architecture: 68k
    environment: >-
      Optimized local release preview in ordinary Chrome with an isolated worker and
      WebGL. This architecture's intact original archive supplied through controlled
      preview responses with SHA-256 and size verification. Ordinary character creation,
      descent to 50 feet, west/east movement and ascent back to town verified in
      inspected captures. Public hosting and release verification pending. Combat, full
      levels, sustained play, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4405
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 4dc3f680e66aaa57769c086c348dd922193914a0
    architecture: ppc
    environment: >-
      Optimized local release preview in ordinary Chrome with an isolated worker and
      WebGL. This architecture's intact original archive supplied through controlled
      preview responses with SHA-256 and size verification. Ordinary character creation,
      descent to 50 feet, west/east movement and ascent back to town verified in
      inspected captures. Public hosting and release verification pending. Combat, full
      levels, sustained play, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4405
runtime:
  show_menu_bar: true
artifacts:
- id: classic
  role: archive
  format: sit
  source:
    type: sha256
    sha256: e37f6ca2500a92c0aa8c65c7dff765a7868e33910d8699fc5aeaae792c03f34d
    size_bytes: 336502
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/a/
    - https://github.com/benletchford/systemless/issues/4405
    license: Historical Angband nonprofit distribution terms
    rights_holder: Angband contributors, James E. Wilson and Robert A. Koeneke
    permission: >-
      The original bundled lib/help/general.txt permits distribution subject to
      retained existing notices and incorporated-code restrictions. lib/help/version.txt
      permits educational, research and nonprofit copying with the copyright and statement
      included.
    notes: >-
      Complete unchanged original port, with its original help, data, preferences and
      notices retained. The separately staged intake copy was freshly downloaded and
      independently verified against the original hash and size. No repack or licence
      substitution.
- id: powerpc
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 91ce28a571189fff8401b2614264c1df47b561dc9e52d14f944e1f17c4df9ffe
    size_bytes: 400492
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/a/
    - https://github.com/benletchford/systemless/issues/4405
    license: Historical Angband nonprofit distribution terms
    rights_holder: Angband contributors, James E. Wilson and Robert A. Koeneke
    permission: >-
      The original bundled lib/help/general.txt permits distribution subject to
      retained existing notices and incorporated-code restrictions. lib/help/version.txt
      permits educational, research and nonprofit copying with the copyright and statement
      included.
    notes: >-
      Complete unchanged original port, with its original help, data, preferences and
      notices retained. The separately staged intake copy was freshly downloaded and
      independently verified against the original hash and size. No repack or licence
      substitution.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 680521737c68fa4696a682e248c4c292c5183312ef8997e14df15f96cfd89572
    size_bytes: 2870
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4405
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Actual 68K native gameplay at 50 feet, cropped to the game window content
      without host UI or menu bar.
---

![Angband dungeon gameplay](https://assets.systemless.org/catalogue/media/sha256/68/680521737c68fa4696a682e248c4c292c5183312ef8997e14df15f96cfd89572.png)

Play the original **Angband 2.7.8** ports as one game. The 68K and PowerPC
choices select their separate original archives.

Choose **New** from **File**, then create a character. **M** selects male and
**F** female; choose the offered race and class letters. **N** uses ordinary
stat rolling. **Escape** accepts the rolled stats, then enter a name and use
**Escape** to continue. The player starts on the town staircase: **>** descends,
**<** ascends, and **Space** acknowledges messages. Numeric **4**, **6**, **8**
and **2** move west, east, north and south.

Native and browser verification covers character creation, town movement, descent
to 50 feet and dungeon movement. Browser checks also cover ascent back to town.
Public release verification is pending.
Combat, full levels, sustained play, saves and audio remain unverified.

The historical educational, research and nonprofit distribution terms apply;
the original notices remain bundled with each archive.
