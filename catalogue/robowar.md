---
id: robowar
kind: game
title: RoboWar
summary: >-
  Design, equip, and program intelligent robotic gladiators in a custom assembly
  language to compete in arena duels and multi-bot battle tournaments.
developer: Lucas Dixon, David Harris
publisher: Lucas Dixon, David Harris
year: 1997
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-17
    tester: Catalogue maintainer
    systemless_version: 0.41.9
    architecture: 68k
    environment: Deterministic headless run from the Info-Mac mirror distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/188
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 058096a482edf0253f824a5da024e0b5c53134e9978aab667d5b5ec23f95173d
    size_bytes: 1244140
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/robowar-44.hqx
    - https://robowar.sourceforge.net/
    license: GNU General Public License v2.0
    rights_holder: Lucas Dixon, David Harris
    permission: "The documentation and source code grant rights under the GNU General Public License: 'RoboWar is free software; you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation; either version 2 of the License, or (at your option) any later version.'"
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 1,244,140-byte file has SHA-256
      058096a482edf0253f824a5da024e0b5c53134e9978aab667d5b5ec23f95173d. Originally created by David Harris and maintained by
      Lucas Dixon, RoboWar was released as free software under the GNU General Public
      License.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 152b5e869bbafdfa7d0b0b530122a7df1169e025a25d51f34f3feae8989e130e
    size_bytes: 7573
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/188
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-17, cropped exactly to the game's 640-by-480 content. It excludes the Classic Mac
      menu bar, browser, website, host desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/robowar-44.hqx
- https://robowar.sourceforge.net/
---

## Robotic gladiators in the digital arena

![RoboWar gameplay](https://assets.systemless.org/catalogue/media/sha256/15/152b5e869bbafdfa7d0b0b530122a7df1169e025a25d51f34f3feae8989e130e.png)

RoboWar challenges players to construct, outfit, and code autonomous combat bots that battle inside
an electronic colosseum. Rather than piloting machines in real time with a joystick or keyboard,
commanders program their creations using RoboTalk—a specialized stack-oriented assembly language
tailored for tactical decision-making, radar scanning, movement vectors, and weapon selection.

Once programmed, bots enter the arena where up to six combatants fight simultaneously in duels or
free-for-all melees. Missiles, laser bursts, collision physics, and defensive shields clash at
custom simulation speeds while the game tracks damage, ammunition, heat levels, and score metrics.

## An enduring classic of open Macintosh gaming

Originally devised by David Harris in the early 1990s and subsequently maintained and expanded by
Lucas Dixon, RoboWar fostered an active global community of roboticists who traded bot scripts and
competed in internet-wide leagues. The project was subsequently open-sourced under the GNU General
Public License.

This entry provides the fat-binary RoboWar 4.4 package preserved in the Info-Mac archive. Systemless
decodes the BinHex format, unpacks the StuffIt archive, and runs the application directly in authentic
classic Macintosh emulation without requiring external ROMs or system installations.
