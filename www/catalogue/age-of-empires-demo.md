---
id: age-of-empires-demo
kind: game
title: Age of Empires Demo
summary: >-
  Lead an ancient civilisation from the Stone Age through combat scenarios in
  the Macintosh demo of Ensemble Studios' real-time strategy classic.
developer: Ensemble Studios
publisher: MacSoft
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: f5ff7e7b deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged demo archive ran natively on the PowerPC slice. Single
      Player, Campaign, a player name, the Armies at War campaign and the Bronze
      Age Art of War briefing led to the live Egyptian map with chariots,
      soldiers and the minimap. Clicking a chariot selected it, showing its
      health bar, outline and unit panel (Egyptian Chariot 133/133). A matched
      control without the click was identical until the input and then differed
      by 20,391 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. Commands, combat, the second campaign, audio and browser
      performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5471
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: f5ff7e7b browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs
      of a locally served trunk build used the exact pinned original archive and
      loaded it to the title and menu screens. Twenty-second samples reached
      61.05, 61.00 and 60.06 guest ticks per second with no console errors.
      Browser input through to gameplay was not scripted; the native run above
      covers gameplay. This approves bounded browser launch.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5471
runtime:
  executable_path: Age of Empires/Age of Empires Trial
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Age%20of%20Empires.sit
    expected_sha256: 798ec5efcdb1514781f96a7b206c543107d44205295494e3a06f8bce3cf43289
    expected_size: 17462318
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/age-of-empires
    - https://download.classicmacdemos.com/Age%20of%20Empires.sit
    - https://github.com/benletchford/systemless/issues/5471
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Microsoft / Ensemble Studios; MacSoft
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under
      the policy for unchanged, freely circulated promotional demos. The
      unchanged demo appeared on at least 17 period Macintosh magazine cover
      discs. The bundled documents contain no explicit redistribution grant or
      restriction. Preserve the complete unchanged archive and remove it if a
      rights holder objects.
    notes: >-
      PowerPC trial build with the demo campaigns Armies at War and Reign of the
      Hittites; Random Map and full campaigns are locked.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/age-of-empires-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5471
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native PowerPC Egyptian map after selecting a chariot.
references:
- https://classicmacdemos.com/age-of-empires
- https://github.com/benletchford/systemless/issues/5471
---

## Build an empire

![Age of Empires Demo](incoming/age-of-empires-demo/gameplay.png)

Command Egyptian chariots and soldiers against the Greeks in the demo's combat showcase, or play the Reign of the Hittites campaign.

Choose **Single Player**, then **Campaign**, enter a name and pick a scenario. Click a unit to select it and right-click or click the map to order it.
