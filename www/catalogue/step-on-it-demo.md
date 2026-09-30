---
id: step-on-it-demo
kind: game
title: Step On It! Demo
summary: Guide Ted through block mazes, collecting a key to reach the exit.
developer: Casady & Greene
publisher: Casady & Greene
year: 1996
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
      Replayed the unchanged StuffIt demo, started Board 001, and held the
      documented L key to move Ted right compared with a matched no-input run.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3652
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + release-mode browser build
    architecture: 68k
    environment: >-
      In Chrome, fetched the unchanged archive once, clicked Start, reached
      Board 001, and moved Ted with the documented L key. A five-second active
      sample measured 59.8 host frames/s and 59.8 guest ticks/s, with a
      24.8 ms maximum frame and 293 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3652
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Step%20On%20It%20Demo.sit
    expected_sha256: cfcd26cbe874c5d64b3f27a54ebe62864e7e3b69f02db4dedf986668a60a19a5
    expected_size: 1737871
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/step-on-it
    rights_holder: Casady & Greene
    permission: >-
      The unchanged archive contains Step On It! DEMO and its ReadMe, which
      expressly identifies the demonstration version and promotes the full game.
      No bundled redistribution restriction was found; no retail application is included.
    notes: >-
      Original 1,737,871-byte StuffIt archive, SHA-256
      cfcd26cbe874c5d64b3f27a54ebe62864e7e3b69f02db4dedf986668a60a19a5.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/step-on-it-demo/screenshot.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3652
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
references:
- https://classicmacdemos.com/step-on-it
---

## Build a path to the key

![Step On It! first board](incoming/step-on-it-demo/screenshot.png)

Click Start to enter the first board. By default, J and L run left and right,
I jumps, A and Z switch blocks, and S and X use weapons. You can inspect or
change these keys in Options → Set Keys. Collect the key, then reach the door.
