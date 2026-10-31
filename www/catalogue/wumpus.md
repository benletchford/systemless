---
id: wumpus
kind: game
title: Wumpus! 1.0
summary: >-
  Explore caverns with a lantern, dynamite and a canary as you track down the
  legendary Wumpus in Joe Strout's graphical take on Hunt the Wumpus.
developer: Joe Strout (Codenautics)
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 0fd116b1 deterministic play runner
    architecture: ppc
    environment: >-
      The original Info-Mac archive ran natively in headless mode and opened a game
      in the first cavern. Holding Up moved the explorer through several caverns,
      revealing the map and using lantern oil. A matched control without the key
      was identical until the input and then differed by 11,358 pixels; Left,
      Right and Down were blocked from the starting cavern. An exact replay matched
      both checkpoints, with zero exhausted frames. Hazards, dynamite, shooting and
      audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5443
runtime:
  executable_path: Wumpus
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/wumpus-10.hqx
    expected_sha256: d623afcbe9eac142bde1094025814ba50ace6095d3013226fa56cfcf0fb9db71
    expected_size: 1065335
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/adv/wumpus-10.hqx
    - https://github.com/benletchford/systemless/issues/5443
    license: Original freeware distribution notice
    rights_holder: Joe Strout (Codenautics)
    permission: >-
      The author's Info-Mac submission says Wumpus is freeware and may be
      distributed at will, including on CD-ROM archives, as long as the program
      and its resources are not modified. Preserve the unchanged archive.
    notes: The archive contains only the PowerPC application.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/wumpus/caverns.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5443
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native PowerPC caverns after moving north, cropped to the game window.
references:
- https://github.com/benletchford/systemless/issues/5443
---

## Hunt the Wumpus

![Wumpus caverns](incoming/wumpus/caverns.png)

Wander the caverns with a single tranquilliser dart, a lantern, a few sticks of
dynamite and a canary. Read the clues, avoid the hazards and get the Wumpus before
it gets you.

Use the arrow keys to move between caverns. Hold Command and press an arrow to
shoot, and hold Shift and press an arrow to blast with dynamite.
