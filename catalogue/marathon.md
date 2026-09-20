---
id: marathon
kind: game
title: Marathon
summary: >-
  Explore a besieged colony ship and fight alien forces in Bungie's first-person
  science-fiction adventure.
developer: Bungie
publisher: Bungie
year: 1994
architectures:
- 68k
- ppc
default_architecture: 68k
category: FPS
launch_enabled: true
controls:
  arrows_as_numpad: true
  mobile:
    enabled: true
    buttons:
    - label: "A"
      key: "Space"
    - label: "B"
      key: "Tab"
    - label: "C"
      key: "Enter"
compatibility:
  status: playable
  verified:
  - date: 2026-09-13
    tester: Catalogue maintainer
    systemless_version: 0.40.0
    architecture: 68k
    environment: Local website preview in the Codex in-app browser
    status: playable
    evidence: >-
      https://github.com/benletchford/systemless.org/blob/master/entries/marathon.md
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: a8ddcadc6f193fb10901433eb8958fe97898a692182bae584e351f40147d2489
    size_bytes: 5627676
  provenance:
    original: true
    redistribution: permitted
    sources:
    - https://macintoshgarden.org/games/marathon
    - https://trilogyrelease.bungie.org/faq.html
    rights_holder: Bungie
    permission: >-
      Bungie authorizes free distribution of the original Marathon trilogy; copyright
      is retained and commercial sale is not permitted.
    notes: "The Trilogy Release FAQ documents the original games’ free-distribution permission. Retrieved 2026-09-13: 5627676 bytes; SHA-256 a8ddcadc6f193fb10901433eb8958fe97898a692182bae584e351f40147d2489."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 9fbf09f713f7c5fbbaf977f5be5ac2c3b5ccd27a85bebd3d508bbb5a00d0ca24
    size_bytes: 221949
  provenance:
    content_only: true
    redistribution: permitted
    sources:
    - https://github.com/benletchford/systemless.org/tree/master/entries
    permission: >-
      Original screenshot captured for this catalogue at the maintainer’s request.
      Underlying game artwork remains the property of its respective rights holders.
    notes: >-
      Fresh unedited gameplay capture from the independently downloaded archive on
      2026-09-13, using Systemless commit e0341ec293c038e5d0faa4d6e26cf05075b7144b after
      the Window Manager fix in https://github.com/benletchford/systemless/pull/1703.
      The gameplay scene and clean borders were visually reviewed; this is not a full
      compatibility assessment.
references:
- https://macintoshgarden.org/games/marathon
---

## Source and version

This entry uses the freely distributable [original Marathon archive](https://assets.systemless.org/catalogue/objects/sha256/a8/a8ddcadc6f193fb10901433eb8958fe97898a692182bae584e351f40147d2489.sit), unchanged from the classic Macintosh release.

## Gameplay

![Marathon gameplay](https://assets.systemless.org/catalogue/media/sha256/9f/9fbf09f713f7c5fbbaf977f5be5ac2c3b5ccd27a85bebd3d508bbb5a00d0ca24.png)

## Aboard the Marathon

The UESC Marathon hangs above Tau Ceti with its crew under attack and its three shipboard AIs pulling you in different directions. The corridors are spare, lonely places: exploration, switches, lifts and terminal messages matter as much as a quick trigger. Reading those terminals is the game; Durandal's voice slowly turns a rescue mission into something more unsettling.
