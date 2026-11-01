---
id: star-trek-25th-anniversary-demo
kind: game
title: "Star Trek: 25th Anniversary Demo"
summary: >-
  Command the starship Enterprise through a mock battle against the U.S.S.
  Republic in Interplay's adventure and starship combat game.
developer: Interplay Productions
publisher: Interplay Productions
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice and reached the
      Enterprise bridge with the Captain's Log. Pressing Escape advanced the log to
      Lieutenant Uhura's report. A matched idle control was identical until the input and then
      differed by 57,008 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. Browser pacing at 10 MHz passed three samples on the bridge at
      about 60 host fps and 60 guest ticks/s. Starship combat, away missions and audio
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5491
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: da00d1cd3fa4db2ce5cbd5f8ba57957964f92e709f7081b2646b4d39b3b3bb06
    size_bytes: 1056219
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/star-trek-25th-anniversary
    - https://download.classicmacdemos.com/Putt-Putt%20Moon.sit
    - https://github.com/benletchford/systemless/issues/5491
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Interplay Productions
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: 68K demo of the Macintosh port of the Star Trek adventure.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7c26d388f56280039aa1a55eccd7c0fce63540c51fe7776cbda17117828de151
    size_bytes: 23682
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5491
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K bridge scene, cropped to the game window.
references:
- https://classicmacdemos.com/star-trek-25th-anniversary
- https://github.com/benletchford/systemless/issues/5491
---

## Mock battle

![Star Trek: 25th Anniversary Demo](https://assets.systemless.org/catalogue/media/sha256/7c/7c26d388f56280039aa1a55eccd7c0fce63540c51fe7776cbda17117828de151.png)

Captain Kirk takes the Enterprise into a training exercise against the U.S.S. Republic. Weapons are set to minimal strength while the ship's computers simulate the damage.

Click on the bridge crew to give orders. Press Escape to advance dialogue.
