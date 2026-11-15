---
id: diamonds-demo
kind: game
title: Diamonds 2.0 Demo
summary: Guide a bouncing ball through Diamonds' colorful block puzzles.
developer: Varcon Systems
publisher: das softwarehaus GmbH
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
native:
  enabled: true
  edition: Demo
  limits: >-
    Original Diamonds 2.0 demonstration. Keep the complete package and its original
    notices.
compatibility:
  status: playable
  verified:
  - date: "2026-11-15"
    tester: Catalogue maintainer
    systemless_version: c3b8417e8a9841f91047d011bc91009affc62e8f
    architecture: 68k
    environment: >-
      Native structured playback of the unchanged StuffIt demo at 800 by 600.
      Verified the restored illuminated border, score, level name, time bar, and ball counter
      after fixing the Exp1to3 stack corruption during music loading. The ball animates
      and responds to Left and Right; sampled audio is non-silent.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6293
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 694fca12 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo. Clicked Play and
      reached the live board. Holding the browser Right Arrow lit the red direction
      indicator while the blue ball moved. One exact archive fetch. A 10-second active-board
      sample measured 60.0 host FPS, 60.3 guest ticks per second, maximum measured
      frame 24.4 ms, and minimum audio queue 138 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3591
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo at 800 by 600. Clicked Play on the launcher
      and reached the live block board. Right, Down, and Space inputs moved the blue
      ball and changed the on-screen control indicators.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3591
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: dfec8d30230938fd08e6da101956a8cee7f1f7f2a64282a09275f1eea910d9be
    size_bytes: 184315
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/diamonds
    rights_holder: Varcon Systems / das softwarehaus GmbH
    permission: >-
      The original Diamonds 2.0 Demo launcher states, "Diese Demo-Version kann
      unbeschränkt kopiert und weitergegeben werden" (this demo version may be copied and
      passed on without restriction). The archive contains only the demo application and
      its two background files.
    notes: >-
      Unchanged 184,315-byte StuffIt archive, SHA-256
      dfec8d30230938fd08e6da101956a8cee7f1f7f2a64282a09275f1eea910d9be. The German demo splash credits das
      softwarehaus GmbH as its distributor.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 744cb1d256ac1e87b09ba76d9a99c65939ebe76e5bdabe369fe41e3824e99afc
    size_bytes: 8469
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6293
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      638-by-458 native capture of the live board with its restored border and status
      graphics. The crop excludes Mac desktop, menu bar, and window frame. PNG SHA-256
      744cb1d256ac1e87b09ba76d9a99c65939ebe76e5bdabe369fe41e3824e99afc, 8,469 bytes.
references:
- https://classicmacdemos.com/diamonds
---

## Diamonds in motion

![Diamonds block board](https://assets.systemless.org/catalogue/media/sha256/74/744cb1d256ac1e87b09ba76d9a99c65939ebe76e5bdabe369fe41e3824e99afc.png)

Choose **Play** on the demo launcher. Use **Left** and **Right** to guide
the bouncing ball through the colored block field. Press **P** to pause,
**R** to restart the level, or **Q** to end the current game.
