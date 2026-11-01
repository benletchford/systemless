---
id: sub-battle-simulator-demo
kind: game
title: Sub Battle Simulator Demo
summary: >-
  Command a World War II submarine from the conning tower, juggling periscope,
  radar, sonar and torpedoes in Epyx's naval simulator.
developer: Epyx
publisher: Epyx
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice and started a mission at
      Battle Stations. Clicking SCOPE and then RADAR switched the right-hand view to the
      radar display with its sweep. A matched idle control was identical until the input
      and then differed by 82,238 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. Torpedo attacks, later missions, and audio remain unverified.
      With 6 MHz pacing, three browser samples during the mission passed at about 60 host fps
      and 60 guest ticks/s; default pacing reached only about 19 ticks/s.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5501
runtime:
  show_menu_bar: true
  runtime_pacing:
    cpu_mhz: 6
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Sub%20Battle%20Simulator%20_.sit
    expected_sha256: d0e83fd0719d184812a13a3c3a166dec98a4c865b341b473613fe1da78c0b5c2
    expected_size: 1244361
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/sub-battle-simulator
    - https://download.classicmacdemos.com/Sub%20Battle%20Simulator%20_.sit
    - https://github.com/benletchford/systemless/issues/5501
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Epyx
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: >-
      68K demo of Epyx's World War II submarine simulator.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/sub-battle-simulator-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5501
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K radar view during a mission, cropped to the game window.
references:
- https://classicmacdemos.com/sub-battle-simulator
- https://github.com/benletchford/systemless/issues/5501
---

## Battle stations

![Sub Battle Simulator Demo](incoming/sub-battle-simulator-demo/gameplay.png)

Lookouts have spotted a contact. Switch between periscope, binoculars, sonar and radar, then line up a torpedo shot before the escorts find you.

Click the station buttons to change views, and use the controls along the bottom to dive, fire and manage power.
