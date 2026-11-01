---
id: the-battle-of-britain-ii-demo
kind: game
title: The Battle of Britain II Demo
summary: >-
  Command RAF Fighter Command in 1940, scrambling squadrons against incoming
  Luftwaffe raids from a plotting map of south-east England.
developer: Deadly Games
publisher: Deadly Games
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive refuses to run above 16 colours, so it ran natively on the
      68K slice at a 4-bit screen depth. It reached the RAF briefing, and a click advanced to
      the south-east England campaign map with airfields, sectors and the plotting clock. A
      matched idle control was identical until the input and then differed by 173,884
      pixels. An exact replay matched both checkpoints, with zero exhausted frames. Raids,
      interceptions, audio remain unverified. In the browser
      the briefing ran at 60 fps but clicks did not advance to the map, so launch stays
      disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5518
runtime:
  screen_depth: 4
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Battle%20of%20Britain%20II.sit
    expected_sha256: 73dac15f47f1e7aeff6ebb9b6a775f4311f489d15115ec1fa0b8ab06088bd94a
    expected_size: 465122
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/the-battle-of-britain-ii
    - https://download.classicmacdemos.com/Battle%20of%20Britain%20II.sit
    - https://github.com/benletchford/systemless/issues/5518
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Deadly Games
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: 68K demo of the 1992 air-war strategy game; it requires a 16-colour display.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/the-battle-of-britain-ii-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5518
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K campaign map at 16 colours, cropped to the game window.
references:
- https://classicmacdemos.com/the-battle-of-britain-ii
- https://github.com/benletchford/systemless/issues/5518
---

## Scramble

![The Battle of Britain II Demo](incoming/the-battle-of-britain-ii-demo/gameplay.png)

Radar stations along the coast report incoming raids. Watch the plotting map, judge the threat and send fighter squadrons up in time to meet the bombers.

Click on the map and use the File and Report menus to manage the battle.
