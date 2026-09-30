---
id: macattack-demo
kind: game
title: MacAttack Demo
summary: Steer a ship through New Reality's fast 3D arcade gauntlet.
developer: New Reality Entertainment, Inc.
publisher: GameTek
year: 1994
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
    systemless_version: 0.70.1 + deterministic play runner and release browser
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo and launched the pinned archive once in the
      release browser. Accepted the TurboCharge prompt, dismissed the sound warning to
      continue silently, then used File > New Game to reach the live 3D playfield.
      Mouse movement steered the ship; Shift consumed a smart bomb. A three-second
      active-play sample measured 60.0 host FPS and 60.3 guest ticks/sec, with a 6 ms maximum
      frame and 131 ms minimum audio queue. The direct 800-by-578 gameplay capture is
      the catalogue screenshot.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3630
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 0367d87cc9cc84df5aa28085834ad6c7a36d70325f2e07060d4abd18581f40f0
    size_bytes: 848461
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/macattack
    - https://static.classicmacdemos.com/demos/macattack/README.txt
    rights_holder: New Reality Entertainment, Inc. / GameTek
    permission: >-
      The original ReadMe expressly invites copying the demo to friends, stores, and
      bulletin boards. This unchanged archive contains the purpose-built three-level
      demo, its game data, and bundled Sound Manager files, rather than the 30-level
      commercial game.
    notes: >-
      Unchanged 848,461-byte StuffIt archive, SHA-256
      0367d87cc9cc84df5aa28085834ad6c7a36d70325f2e07060d4abd18581f40f0.
      Fetched the promoted public archive and gameplay screenshot; both matched
      their pinned SHA-256 hashes and byte lengths.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 72e406064f2b5228d1127eed44977c0a625fbe54f9bd8e6d4b02745dbd0dd68b
    size_bytes: 27971
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3630
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
references:
- https://classicmacdemos.com/macattack
- https://static.classicmacdemos.com/demos/macattack/README.txt
---

## Navigate the Net

![MacAttack playfield](https://assets.systemless.org/catalogue/media/sha256/72/72e406064f2b5228d1127eed44977c0a625fbe54f9bd8e6d4b02745dbd0dd68b.png)

Accept the first-run TurboCharge prompt. If a sound error appears, click Okay
to continue without sound. Use **File > New Game** to start, then move the mouse
to steer. Space activates the shield and Shift uses a smart bomb. The original
demo contains three selected levels.
