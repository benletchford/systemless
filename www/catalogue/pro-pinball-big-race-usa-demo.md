---
id: pro-pinball-big-race-usa-demo
kind: game
title: 'Pro Pinball: Big Race USA Demo'
summary: >-
  Race a taxi across America on a high-resolution 3D pinball table with
  realistic ball physics, ramps and video modes.
developer: Cunning Developments
publisher: Empire Interactive
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged demo archive ran natively on PowerPC and reached the 3D table in
      attract mode after Return three times. Pressing S, the game's Start key, started a game
      and the meter read $11,000. A matched idle control was identical until the input and
      then differed by 24,309 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. Flipper play, scoring, audio remain unverified. With 30 MHz pacing
      (adaptive minimum 12), three browser samples on the table passed at 56.5–59.8 host fps;
      default pacing reached 51–53 fps.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5503
runtime:
  runtime_pacing:
    cpu_mhz: 30
    adaptive_min_cpu_mhz: 12
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Big%20Race%20USA.sit
    expected_sha256: 59ff203ad91670446de9e5f46afae55be4302733de66dacc99457d5f39760420
    expected_size: 26623475
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/pro-pinball-big-race-usa
    - https://download.classicmacdemos.com/Big%20Race%20USA.sit
    - https://github.com/benletchford/systemless/issues/5503
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Empire Interactive
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: >-
      PowerPC demo of the third Pro Pinball table. The Start key is S; the flippers
      and plunger use the keyboard bindings shown in the game's options.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/pro-pinball-big-race-usa-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5503
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native PowerPC table after starting a game, cropped to the game window.
references:
- https://classicmacdemos.com/pro-pinball-big-race-usa
- https://github.com/benletchford/systemless/issues/5503
---

## Coast to coast

![Pro Pinball: Big Race USA Demo](incoming/pro-pinball-big-race-usa-demo/gameplay.png)

Drive a cab across the United States by hitting ramps, targets and the city map on a detailed pinball table. Multiball, video modes and a dot-matrix display complete the arcade feel.

Press S to start. Use the keyboard flipper and plunger keys set in the options; Return launches the ball.
