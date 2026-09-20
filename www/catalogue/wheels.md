---
id: wheels
kind: game
title: Wheels!
summary: Race through an accessible first-person maze built around keyboard-only wheelchair controls.
developer: RJ Cooper & Assoc.
publisher: RJ Cooper & Assoc.
year: 1998
architectures: [68k]
default_architecture: 68k
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.1
    architecture: 68k
    environment: Deterministic headless gameplay run from the original StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/wheels/
    download_page: https://classicmacdemos.com/wheels
    expected_sha256: 7fa61846ef8c26015b7570f0e97b834de1fd4b57ddb72429aab3c26ea2744f1a
    expected_size: 1723677
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/wheels
    - https://static.classicmacdemos.com/demos/wheels/README.txt
    license: Wheels! demo sharing permission
    rights_holder: RJ Cooper & Assoc.
    permission: >-
      The included README expressly encourages copying the demo for friends
      while distinguishing it from the non-copyable full version.
    notes: >-
      Unchanged 1,723,677-byte demo archive, SHA-256
      7fa61846ef8c26015b7570f0e97b834de1fd4b57ddb72429aab3c26ea2744f1a.
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
