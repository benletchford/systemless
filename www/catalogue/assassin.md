---
id: assassin
kind: game
title: Assassin 1.1.2
summary: >-
  Plot simultaneous moves and shots through a maze to hunt down rival assassins in
  Steven H. Taylor's turn-based networked strategy game.
developer: Steven H. Taylor
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 52de7794 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. Choosing Solo
      and accepting Game Setup opened the Dead Zone maze against two computer
      players. Clicking the on-screen Up and Right move buttons entered moves and
      moved the player's marker. A matched control without the clicks was identical
      until the input and then differed by 238 pixels. An exact replay matched every
      checkpoint, with zero exhausted frames. Keyboard move entry was not accepted
      in this run. Turn resolution, network play, audio and browser performance
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5425
runtime:
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/assassin-112.hqx
    expected_sha256: 7ca8caa8df392db268823df910de0cf0732af77ea20d54681fc08b0b31a2377f
    expected_size: 517649
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/assassin-112.hqx
    - https://github.com/benletchford/systemless/issues/5425
    license: Original free distribution notice
    rights_holder: Steven H. Taylor
    permission: >-
      The bundled Read Me permits copying and distributing the software freely, as
      long as that Read Me is included. Modified copies and charging for the
      software or its distribution are not permitted. Preserve the complete
      unchanged archive.
    notes: The archive contains the 68K application, instructions and Read Me.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/assassin/maze.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5425
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K Dead Zone maze during move entry, cropped to the play field.
references:
- https://github.com/benletchford/systemless/issues/5425
---

## Plan your strike

![Assassin Dead Zone maze](incoming/assassin/maze.png)

Every player secretly plots a turn of moves and shots. All the moves then play
out at once. Hunt your target through the maze while avoiding the assassin hunting
you.

Choose **Solo** and click **OK**, then **OK** on Game Setup. Click the arrow, gun
and look buttons below the maze to enter moves, then click the cycle arrow to
play the turn.
