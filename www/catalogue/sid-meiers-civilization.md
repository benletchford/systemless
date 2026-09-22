---
id: sid-meiers-civilization
kind: game
title: Sid Meier's Civilization
summary: Guide a Roman civilization through cities, research and exploration in MicroProse's playable strategy demonstration.
developer: MicroProse
publisher: MicroProse
year: 1992
architectures: [68k]
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: 2026-09-22
    tester: Catalogue maintainer
    systemless_version: 0.48.0
    architecture: 68k
    environment: >-
      Deterministic gameplay run from the unchanged original StuffIt demo
      archive, cross-checked through the same 28-action script under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Civilisation%20Demo.sit
    expected_sha256: 49fcc1db305f8dd40e4ee782ed9caf10832d36a897ad4727c0e628d1ee6a4f26
    expected_size: 931124
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/sid-meiers-civilization
    - https://static.classicmacdemos.com/demos/sid-meiers-civilization/README.txt
    - https://civilization.2k.com/
    license: MicroProse Civilization Demo promotional distribution
    rights_holder: Take-Two Interactive Software, Inc.
    permission: >-
      MicroProse packaged this as a deliberately limited demonstration taken
      from the retail game. Its enclosed ReadMe invites players to enjoy the
      demo and directs them to MicroProse or a retailer for the full product.
      The unchanged package was distributed on at least eleven contemporary
      magazine, student-resource and promotional discs. That documented
      promotional distribution supports continued redistribution of this exact
      demo package; it does not extend to the retail game or modified copies.
    notes: >-
      The unchanged 931,124-byte StuffIt archive has SHA-256
      49fcc1db305f8dd40e4ee782ed9caf10832d36a897ad4727c0e628d1ee6a4f26.
      It contains a 1,982-byte MicroProse ReadMe and the 1,253,156-byte 68K
      Civilization Demo application with its resource fork. The current 2K
      franchise site identifies the original 1991 Civilization as part of the
      continuing series. The catalogue hosts only this promotional demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/sid-meiers-civilization/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2222
    permission: >-
      Original gameplay screenshot captured for this catalogue at the
      maintainer's request. Underlying game artwork remains the property of
      Take-Two Interactive Software, Inc.
    notes: >-
      Fresh deterministic Systemless 0.48.0 capture made from the exact
      unchanged demo archive on 2026-09-22 after entering the Roman scenario.
      The 800x600 framebuffer was cropped to the 585x540 Local Map content
      surface, excluding the Mac menu bar, window title, scrollbars and host
      framing; no game pixels were altered. PNG SHA-256
      dad03e39586b9a68d2ba86685cef286f883926aa6e668fe4316242cda230e8c3,
      142,849 bytes.
references:
- https://classicmacdemos.com/sid-meiers-civilization
- https://static.classicmacdemos.com/demos/sid-meiers-civilization/README.txt
- https://civilization.2k.com/
---

## An empire already in motion

![Sid Meier's Civilization gameplay](incoming/sid-meiers-civilization/gameplay.png)

The demo opens in 2100 BC with a Roman civilization spread across a revealed
island. London and Caesarea anchor the map, while chariots, settlers and other
units wait for orders among roads, forests, hills and coast. The compact
scenario exposes the decisions that define Civilization: explore the terrain,
inspect cities, balance their production and decide where each unit should go.

MicroProse describes the demonstration as a limited selection taken from the
retail game. City screens expose population, food, trade, taxes, construction
and troop support; reports show the civilization's progress; and the
Civilopedia retains descriptions of technologies, wonders, units and city
improvements even though many of its retail illustrations were omitted to keep
the demo small.

## Preserved as MicroProse distributed it

The source package is the original StuffIt archive rather than a retail disk,
crack, repack or generated web bundle. Systemless opens the archive directly,
discovers the application and ReadMe, reaches the live Roman map and can open
the Civilopedia through the game's menu. The same deterministic sequence was
cross-checked under BasiliskII.

Take-Two continues the Civilization series through 2K. This catalogue preserves
only MicroProse's intentionally limited promotional demonstration byte-for-byte;
it does not provide the original retail game. The launcher remains disabled
until the archive and screenshot complete asset promotion and browser review.
