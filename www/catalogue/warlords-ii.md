---
id: warlords-ii
kind: game
title: Warlords II Demo
summary: >-
  Lead armies across a fantasy kingdom in Strategic Studies Group's original
  50-turn Macintosh demonstration.
developer: Strategic Studies Group
publisher: Strategic Studies Group
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
runtime:
  runtime_pacing:
    cpu_mhz: 10
compatibility:
  status: playable
  verified:
  - date: "2026-09-27"
    tester: Catalogue maintainer
    systemless_version: cac423b6c3f8bef4603135bcf56f6684d3c6bf0b
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged Macintosh demo. Started the
      first turn, hired a hero, ordered Scout production, inspected a city, selected an
      army, moved it east, and continued play. The same script completed in BasiliskII.
      Browser launch has not yet been verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2861
  - date: 2026-09-28
    tester: Catalogue maintainer
    systemless_version: 0.65.1
    architecture: 68k
    environment: >-
      Release-mode Chrome 151 browser preview of the checksum-matched original
      demo at the entry's 10 MHz pacing setting. Two standard startup samples
      sustained about 60 host FPS and 60 guest ticks per second; a separate
      interaction run reached turn one, hero hiring and production help.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3025
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 3194164d98a5accd97d0863569d6c0b20e8dc96ba701d5857b71b4d87b670a1b
    size_bytes: 1240783
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/warlords-ii
    - https://static.classicmacdemos.com/demos/warlords-ii/README.txt
    rights_holder: Strategic Studies Group
    permission: "This unchanged promotional Macintosh demo is publicly distributed by Classic Macintosh Game Demos. Its included readme calls it a limited-play demo: one world, at most 50 turns, with saving and loading disabled. No retail game files are substituted. The archive has no express redistribution clause."
    notes: >-
      Original 1,240,783-byte StuffIt archive, SHA-256
      3194164d98a5accd97d0863569d6c0b20e8dc96ba701d5857b71b4d87b670a1b. The tested application is Warlords II Demo, a
      68K executable.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a5690396739a88a74d816c0f5ef6d68b57e475da2b234ef6756ce924b0596e4f
    size_bytes: 204276
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2861
    permission: >-
      Fresh gameplay capture made from the unchanged original demo for this catalogue
      entry. The underlying game artwork remains its owner's property.
    notes: >-
      Exact 800-by-580 crop at (0,20) of a deterministic Systemless 800-by-600
      framebuffer after an army move. Only the Classic Mac menu bar was removed; no game
      pixels were altered. PNG SHA-256
      a5690396739a88a74d816c0f5ef6d68b57e475da2b234ef6756ce924b0596e4f, 204,276 bytes.
references:
- https://classicmacdemos.com/warlords-ii
- https://static.classicmacdemos.com/demos/warlords-ii/README.txt
- https://github.com/benletchford/systemless/pull/2879
---

## A kingdom in fifty turns

![Warlords II demo strategy map after an army move](https://assets.systemless.org/catalogue/media/sha256/a5/a5690396739a88a74d816c0f5ef6d68b57e475da2b234ef6756ce924b0596e4f.png)

The first turn puts a hero in your capital, asks you to choose what the city
will produce, and then opens a map of castles, roads, rivers, and armies. This
capture shows the playable map after moving an army east from its starting
position. Cities and unit controls remain available for the next decision.

This is Strategic Studies Group's original limited Macintosh demonstration,
not the retail release. The included readme limits play to one world and 50
turns; it also disables saving and loading. Systemless has passed a
deterministic gameplay replay and an early-game browser interaction check.
In the browser, dismiss the two introduction panels with Done, choose Begin
Game, then click the turn banner to start giving orders.
