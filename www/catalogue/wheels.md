---
id: wheels
kind: game
title: Wheels!
summary: >-
  Race through an accessible first-person maze built around keyboard-only
  wheelchair controls.
developer: RJ Cooper & Assoc.
publisher: RJ Cooper & Assoc.
year: 1998
architectures:
- 68k
default_architecture: 68k
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-20"
    tester: Catalogue maintainer
    systemless_version: "0.42.1"
    architecture: 68k
    environment: Deterministic headless gameplay run from the original StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 7fa61846ef8c26015b7570f0e97b834de1fd4b57ddb72429aab3c26ea2744f1a
    size_bytes: 1723677
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/wheels
    - https://static.classicmacdemos.com/demos/wheels/README.txt
    license: Wheels! demo sharing permission
    rights_holder: RJ Cooper & Assoc.
    permission: >-
      The included README expressly encourages copying the demo for friends while
      distinguishing it from the non-copyable full version.
    notes: >-
      Unchanged 1,723,677-byte demo archive, SHA-256
      7fa61846ef8c26015b7570f0e97b834de1fd4b57ddb72429aab3c26ea2744f1a.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4ecf167563bc24a015500aad333f02e0c8b3f6c2d77ae64939f601033b1a06c8
    size_bytes: 35728
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2335
    - https://classicmacdemos.com/wheels
    - https://static.classicmacdemos.com/demos/wheels/README.txt
    permission: >-
      Original gameplay screenshot captured from the exact unchanged shareware
      archive for this catalogue entry. Underlying game artwork remains the property of RJ
      Cooper & Assoc.
    notes: >-
      Fresh deterministic Systemless 0.45.0 capture on 2026-09-21 using the exact
      1,723,677-byte archive above. The run entered the live Level 1 first-person maze
      after dismissing its briefing; the 800×600 framebuffer was cropped to the 640×480
      game surface (x=80..720, y=60..540), excluding black emulator margins. PNG SHA-256
      4ecf167563bc24a015500aad333f02e0c8b3f6c2d77ae64939f601033b1a06c8; size 35,728
      bytes.
references:
- https://classicmacdemos.com/wheels
- https://static.classicmacdemos.com/demos/wheels/README.txt
---

## Wheel through the maze

Wheels! is a first-person action maze designed so that players can move, fire
and interact using only the keyboard. The demo includes its easy maze: collect
the key, clear the clowns and find the exit. Systemless launches the original
68k archive, enters Level 1 and responds to movement, fire and activation
controls in a deterministic gameplay run.

![Wheels! gameplay](https://assets.systemless.org/catalogue/media/sha256/4e/4ecf167563bc24a015500aad333f02e0c8b3f6c2d77ae64939f601033b1a06c8.png)
