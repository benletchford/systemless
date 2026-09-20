---
id: prince-of-destruction
kind: game
title: Prince of Destruction
summary: >-
  Explore the realm of Nestaria, assemble a heroic party, and overthrow the
  tyrannical Prince Grishnákh in BadgerCom's classic top-down fantasy RPG.
developer: BadgerCom Software
publisher: BadgerCom Software
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
aliases:
- /pod
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-18
    tester: Catalogue maintainer
    systemless_version: 0.41.9
    architecture: 68k
    environment: Deterministic headless run from the preserved StuffIt distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/259
controls:
  mobile:
    enabled: true
    joystick:
      down: None
      left: "["
      right: \
      up: "]"
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: d2080a5462832e2737ec3f706fb8f8c48c4cfe4782048fa15cf79e5730546e79
    size_bytes: 3235466
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Prince%20of%20Destruction.sit
    - https://macintoshgarden.org/games/prince-of-destruction
    - https://www.macintoshrepository.org/3909-prince-of-destruction
    license: Free License
    rights_holder: BadgerCom Software Inc.
    permission: "Originally released as shareware in 1995 by BadgerCom Software Inc., co-designer and artist Tonio Loewald subsequently released the public Free License (Registration Name: \"Free License\", Serial: 44G66FCD20F5C0ED) to make the full game freely available for personal and community preservation without restriction."
    notes: >-
      The 3,235,466-byte StuffIt distribution archive has SHA-256
      d2080a5462832e2737ec3f706fb8f8c48c4cfe4782048fa15cf79e5730546e79. BadgerCom Software has dissolved
      and there are no active modern commercial releases or storefront listings for this
      title.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c7f653864e1dbf0d283fcef228d0bd5317864e9f278b2fc206b23050c2ae3572
    size_bytes: 302955
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/259
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the distribution archive on 2026-09-18,
      cropped exactly to the game's 640-by-480 content surface. It excludes the Classic Mac
      menu bar, browser, website, host desktop and emulator chrome.
references:
- https://macintoshgarden.org/games/prince-of-destruction
- https://www.macintoshrepository.org/3909-prince-of-destruction
---

## Epic fantasy RPG in the realm of Nestaria

![Prince of Destruction gameplay](https://assets.systemless.org/catalogue/media/sha256/c7/c7f653864e1dbf0d283fcef228d0bd5317864e9f278b2fc206b23050c2ae3572.png)

*Prince of Destruction*, released in 1995 by BadgerCom Software, is a landmark classic Macintosh fantasy role-playing game built atop the proprietary M.A.R.S. (Multi-player Animated Role-playing System) engine. Programmed by Andrew Barry with rich design, hand-drawn graphics, and animation by Tonio and Pamina Loewald, the game transports players to the enchanted kingdom of Nestaria.

Summoned to the pinnacle of a stone monolith by a powerful wizard, the player embarks on an urgent quest to depose the villainous Prince Grishnákh—the self-styled Prince of Destruction—who violently seized the throne from his sister. Players select from distinct archetypes, including the warrior Nanoc, the northern fighter Thysa, the nimble elf Silly, and the mystic Ada, each offering specialized martial proficiencies and magical capabilities.

The gameplay combines real-time exploration, tactical combat, and non-linear world traversal across more than 1,500 interconnected map regions. Players interrogate non-player characters through a classic keyword dialogue system, search ancient ruins, solve environmental puzzles, and manage equipment across multiple weapon and armor slots.

## Native execution

Systemless executes *Prince of Destruction* natively within its classic 68K and PowerPC runtime environment. The runtime resolves M.A.R.S. off-screen 256-color Color QuickDraw blits, dynamic palette switching, sound effects, and custom dialog interfaces with sub-millisecond responsiveness directly in modern web browsers.
