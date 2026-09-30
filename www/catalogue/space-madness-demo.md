---
id: space-madness-demo
kind: game
title: Space Madness Demo
summary: Pilot a spacecraft through the hazards of a distant starfield.
developer: High Risk Ventures
publisher: High Risk Ventures
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, dismissed its speed notice, started a new
      game, and observed a held keypad 6 change the ship pose against a matched
      no-input replay.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3659
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      In Chrome, fetched the unchanged archive once, dismissed the speed notice,
      started a new game, reached live spacecraft play, and sent keypad 6 steering input. A
      five-second active sample measured 60.0 host frames/s, 55.8 guest ticks/s, 14.8
      ms maximum frame, and 126 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3659
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 58fae30f9b217a5f1792bd0f7bdea7675da012e0cf1c67306181458a63362f23
    size_bytes: 609147
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/space-madness
    - https://groups.google.com/g/comp.sys.mac.games/c/3CUSYlNmDgU
    rights_holder: High Risk Ventures
    permission: >-
      The unchanged archive contains only the original 68K demonstration application;
      its title identifies it as a demo, and no bundled redistribution restriction was
      found. High Risk Ventures publicly announced FTP distribution of the demo in
      1993.
    notes: >-
      Original 609,147-byte StuffIt archive, SHA-256
      58fae30f9b217a5f1792bd0f7bdea7675da012e0cf1c67306181458a63362f23.
      The promoted public object was fetched back and matched this size and hash.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4ccd525de32fe4a90d167629cf1f698824b94a6cfe47ac26a60d53dea5f0bdbd
    size_bytes: 13860
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3659
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
references:
- https://classicmacdemos.com/space-madness
---

## Enter the starfield

![Space Madness starfield](https://assets.systemless.org/catalogue/media/sha256/4c/4ccd525de32fe4a90d167629cf1f698824b94a6cfe47ac26a60d53dea5f0bdbd.png)

Dismiss the opening speed notice, then press S to start a new game. The default
controls use keypad 4 and 6 to turn, keypad 5 to thrust, and Command and Option
to fire. Press C on the title screen to view or change the controls.
