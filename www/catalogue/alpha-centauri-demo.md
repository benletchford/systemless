---
id: alpha-centauri-demo
kind: game
title: Sid Meier's Alpha Centauri Demo
summary: >-
  Found the first human colonies on an alien world in the Macintosh demo of
  Brian Reynolds' and Sid Meier's science-fiction strategy game.
developer: Firaxis Games
publisher: Aspyr Media
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
      The unchanged demo archive ran natively on the PowerPC slice. After the
      demo introduction, Quick Start and the Planetfall briefing, confirming the
      default base name founded Gaia's Landing on the rendered planet surface
      and advanced the game to Mission Year 2101. A matched control without the
      click was identical until the input and then differed by 135,277 pixels.
      An exact replay matched both checkpoints, with zero exhausted frames. Unit
      movement, later turns, audio and browser performance remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5472
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: f5ff7e7b browser build with launch flag
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs
      of a locally served trunk build used the exact pinned original archive and
      loaded it to the title and menu screens. Twenty-second samples reached
      60.00, 60.06 and 60.01 guest ticks per second with no console errors.
      Browser input through to gameplay was not scripted; the native run above
      covers gameplay. This approves bounded browser launch.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5472
runtime:
  executable_path: Alpha Centauri 1.4b6/Alpha Centauri DEMO
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Alpha%20Centauri%201.4b6.sit
    expected_sha256: 2ab4f1de7291d3704b08d7b12dd659ef3b4fbab4b10914f31b02c2ccec931191
    expected_size: 18267807
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/sid-meiers-alpha-centauri
    - https://download.classicmacdemos.com/Alpha%20Centauri%201.4b6.sit
    - https://github.com/benletchford/systemless/issues/5472
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Firaxis Games / Electronic Arts; Aspyr Media
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under
      the policy for unchanged, freely circulated promotional demos. The
      unchanged demo appeared on at least 19 period Macintosh magazine cover
      discs. The bundled documents contain no explicit redistribution grant or
      restriction. Preserve the complete unchanged archive and remove it if a
      rights holder objects.
    notes: >-
      PowerPC demo 1.4b6 limited to a 100-turn single-player game with up to 10
      bases and technologies to level 5.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/alpha-centauri-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5472
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native PowerPC planet surface after founding Gaia's Landing.
references:
- https://classicmacdemos.com/sid-meiers-alpha-centauri
- https://github.com/benletchford/systemless/issues/5472
---

## Make planetfall

![Sid Meier's Alpha Centauri Demo](incoming/alpha-centauri-demo/gameplay.png)

Choose a faction and build your first bases on Planet in this 100-turn demo.

Click **OK** on the introduction, choose **Quick Start**, then **OK** on the Planetfall briefing and base-name prompts. Move units with the keypad or mouse, and use **Turn Complete** to end your turn.
