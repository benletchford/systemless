---
id: turtle-dice-20-68k
kind: game
title: Turtle Dice 2.0
summary: Roll five dice, choose which to reroll, and fill a Yahtzee-like scorecard.
developer: Greg Pierce
publisher: Turtle Productions
year: 1999
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.74.0 + Appearance and Control Manager runtime PRs
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened its 68K scorecard. A deterministic replay
      dismissed the shareware notice and rolled five dice. The release browser fetched
      the archive once, dismissed the notice, rolled five dice, and scored Chance for
      11 points; Rolls Remaining reset to 3 for the next turn. A five-second
      active-board sample measured 60.0 host frames/s and 60.2 guest ticks/s, with a 2.7 ms
      maximum frame and 131 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3918
runtime:
  runtime_pacing:
    cpu_mhz: 8
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: f5ef81036f200a7d03602c4b0b987018d30530095595ab1caa56ed20e688ef11
    size_bytes: 670161
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=11456
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/turtle-dice-20-68k.hqx
    rights_holder: Greg Pierce
    permission: >-
      The bundled Read Me permits free distribution provided the original package is
      not altered. This source is the complete original Info-Mac archive.
    notes: >-
      Original 670,161-byte 68K BinHex/StuffIt package, SHA-256
      f5ef81036f200a7d03602c4b0b987018d30530095595ab1caa56ed20e688ef11.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 6881a1c5180ae3f13607001cf6fec3978b11b71510c8250c9a7fa0594c6e853e
    size_bytes: 15570
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3918
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original shareware
      package. Underlying game artwork remains its owner's property.
    notes: >-
      291-by-407 game-board crop from a release-browser capture after dismissing the
      shareware notice, rolling five dice, and scoring Chance for 11 points.
references:
- https://info-mac.org/viewtopic.php?t=11456
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/turtle-dice-20-68k.hqx
---

## Roll and score

![Turtle Dice scorecard after a Chance score](https://assets.systemless.org/catalogue/media/sha256/68/6881a1c5180ae3f13607001cf6fec3978b11b71510c8250c9a7fa0594c6e853e.png)

Choose **Not Yet** on the shareware reminder to play without registering. Click
**Roll** to throw five dice. Click a die to select it for a reroll, then click
**Roll** again; each turn allows two rerolls. Click a scorecard box to record
your chosen category; the browser test scored 11 points in **Chance**. The bundled
Read Me explains each category and permits
redistribution of the unchanged package.
