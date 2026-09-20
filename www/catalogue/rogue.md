---
id: rogue
kind: game
title: Rogue
summary: >-
  Brave procedurally generated dungeon chambers, battle monsters, and retrieve
  the Amulet of Yendor in Rick Holzgrafe's 1994 classic Macintosh port of the
  definitive roguelike.
developer: Ken Arnold, Michael Toy & Glenn Wichman
publisher: Rick Holzgrafe
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-18
    tester: Catalogue maintainer
    systemless_version: 0.41.9
    architecture: 68k
    environment: Deterministic headless run from the Info-Mac mirror distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/274
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 067e2ded61aabce4de85f816d75f94132113e9398654bbc6289acfcb63319725
    size_bytes: 95769
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/rogue.hqx
    license: Freeware
    rights_holder: Ken Arnold, Michael Toy, Glenn Wichman, Timothy Stoehr & Rick Holzgrafe
    permission: "The bundled documentation and Info-Mac submission state: \"In accordance with the UC Berkeley copyright which permits modification and distribution but prohibits profiting from the results, this release is freeware.\""
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 95,769-byte file has SHA-256
      067e2ded61aabce4de85f816d75f94132113e9398654bbc6289acfcb63319725. Ported to the Macintosh by Rick Holzgrafe from Timothy
      Stoehr's UC Berkeley distribution of the 1980 classic.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7b410cb0b5b693b9eb8da932f485e47d0d8f984b9d630293dd2d1d0d368fe30b
    size_bytes: 1563
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/274
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-18, cropped exactly to the application's 409-by-277 content surface. It excludes
      the Classic Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/rogue.hqx
---

## The eponymous dungeon crawl

![Rogue gameplay](https://assets.systemless.org/catalogue/media/sha256/7b/7b410cb0b5b693b9eb8da932f485e47d0d8f984b9d630293dd2d1d0d368fe30b.png)

Created in 1980 by Ken Arnold, Michael Toy, and Glenn Wichman, *Rogue* pioneered procedural dungeon generation, permadeath, and turn-based tactical combat, giving rise to the entire roguelike genre. This 1994 classic Macintosh release, ported by Rick Holzgrafe and based on Timothy Stoehr's BSD release, presents the complete original Unix experience inside a clean, faithful terminal window.

Players assume the identity of an intrepid adventurer descending into the Dungeons of Doom in search of the legendary Amulet of Yendor. Every descent offers newly randomized floor plans, monster placements, scrolls, potions, wands, and armor. Surviving requires careful risk assessment: identifying unidentified magic items, conserving food, balancing aggression against tactical retreat, and utilizing environmental bottlenecks to outwit hostile denizens.

## Classic Macintosh terminal execution

Systemless executes *Rogue* within its Motorola 68k runtime environment. The runtime directly implements the classic Macintosh Window Manager, QuickDraw monospaced text drawing, keyboard event dispatch, and standard C library routines, rendering the classic 24-by-80 dungeon grid with full fidelity directly in modern web browsers.
