---
id: odyssey-the-legend-of-nemesis
kind: game
title: "Odyssey: The Legend of Nemesis"
summary: >-
  Cast away on a savage archipelago, master psionics and forge alliances in
  Richard Rouse III's sprawling, non-linear classic Macintosh fantasy RPG.
developer: Richard Rouse III
publisher: Paranoid Productions
year: 1996
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-17
    tester: Catalogue maintainer
    systemless_version: 0.41.9
    architecture: 68k
    environment: Deterministic headless run from the author-hosted original freeware archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/172
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 7eb18188e7a5b40fc1d93983a53e9c58c501dd9d331bea303dbe60f04fa79d1a
    size_bytes: 7316110
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.paranoidproductions.com/odyssey/
    - https://www.paranoidproductions.com/odyssey/downloads/OdysseyFreeware.sit.hqx
    license: Freeware
    rights_holder: Richard Rouse III / Paranoid Productions
    permission: "The author explicitly grants permission on the official website: \"You are freely allowed to distribute this Stuffit archive far and wide. Enjoy!\" and in the bundled documentation: \"Permission is granted to distribute the accompanying software and files freely in its original Stuffit archive in its original form only and as long as all accompanying documents are included.\""
    notes: >-
      Untouched author-hosted BinHex/StuffIt archive retrieved from
      paranoidproductions.com. The 7,316,110-byte file has SHA-256
      7eb18188e7a5b40fc1d93983a53e9c58c501dd9d331bea303dbe60f04fa79d1a. Richard Rouse III / Paranoid Productions remains the
      sole rights holder and has no active modern commercial releases or storefront
      listings for this title.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 45cdaad2360bb24a2ff4fe97cf0b8f704e8e73fe297d15e68329bffab6aed50b
    size_bytes: 263974
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/172
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged author-hosted archive on
      2026-09-17, cropped exactly to the game's 640-by-480 content. It excludes the Classic
      Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://www.paranoidproductions.com/odyssey/
- https://www.paranoidproductions.com/odyssey/downloads/OdysseyFreeware.sit.hqx
---

## Shipwrecked in the archipelago

![Odyssey: The Legend of Nemesis gameplay](https://assets.systemless.org/catalogue/media/sha256/45/45cdaad2360bb24a2ff4fe97cf0b8f704e8e73fe297d15e68329bffab6aed50b.png)

Washed ashore on the beaches of an unfamiliar island with your possessions lost to
the sea, survival in Odyssey begins with caution. The archipelago is an ancient,
fractured realm inhabited by rival tribes, eccentric scholars, hostile wildlife,
and elusive psionic masters. Rather than relying solely on brute force, progression
demands exploration, conversation, and an understanding of the intricate factions
vying for power across the islands.

Odyssey blends open-world freedom with deep tactical mechanics. Characters develop
disciplines ranging from hand-to-hand combat to potent psionic arts, navigating an
interactive top-down world where every NPC can be engaged in dialogue through a
keyword inquiry system. Quests rarely have simple solutions, offering branching
paths, moral choices, and multiple distinct conclusions that reflect how you shape
the fate of the archipelago.

## The author's freeware release

Programmed by Richard Rouse III and released commercially by MacSoft in 1996,
Odyssey was later made available as a complete freeware release by the author through
Paranoid Productions. The release preserves the original 1996 fat binary, containing
both 68K and PowerPC code alongside historical design retrospectives and novella
materials.

The distribution archive remains completely untouched. Systemless decodes the BinHex
layer, unpacks the internal StuffIt archive, and launches the native application
directly without modifying any game assets.
