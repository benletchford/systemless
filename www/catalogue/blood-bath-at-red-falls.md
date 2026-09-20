---
id: blood-bath-at-red-falls
kind: game
title: Blood Bath at Red Falls
summary: Search a hostile frontier town in UnderWorld Software's first-person action demo.
developer: UnderWorld Software
publisher: UnderWorld Software
year: 1995
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
    environment: Deterministic headless gameplay run from the original version 1.70 demo archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Blood%20Bath%20Demo%201.70.sit
    download_page: https://classicmacdemos.com/blood-bath-at-red-falls
    expected_sha256: ea4b211ee5da6005dfa5739776923de8c288127de5b8018155e13318b1e9e003
    expected_size: 1434552
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/blood-bath-at-red-falls
    - https://static.classicmacdemos.com/demos/blood-bath-at-red-falls/README.txt
    license: UnderWorld Software demo distribution permission
    rights_holder: UnderWorld Software
    permission: >-
      The included README permits the demo to be distributed freely when all
      original files are included and remain unaltered.
    notes: >-
      Unchanged 1,434,552-byte version 1.70 demo, SHA-256
      ea4b211ee5da6005dfa5739776923de8c288127de5b8018155e13318b1e9e003.
references:
- https://classicmacdemos.com/blood-bath-at-red-falls
---

## Trouble in Red Falls

Blood Bath at Red Falls turns a small frontier settlement into a first-person
maze of ambushes and locked passages. The demo targets colour 68k Macs and is
preserved here as UnderWorld Software distributed it.

The current Systemless runtime reaches the live first-person view from the
demo menu. A deterministic gameplay run also fires the pistol and observes the
scene advance afterwards, verifying interaction rather than startup alone.
