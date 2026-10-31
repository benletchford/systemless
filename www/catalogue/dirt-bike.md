---
id: dirt-bike
kind: game
title: Dirt Bike 3.0
summary: >-
  Race a physics-driven motocross bike over jumps and hills, then build your own
  tracks and bikes.
developer: Brad Quick and Michel de Messieres
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 0.102.0 deterministic play runner at d71b3b09
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. It passed the
      unregistered notice and title screen and reached the Standard Track start
      line. Moving the mouse to full throttle and upshifting twice with X rode the
      bike forward. The gauges showed first gear and distance increasing from
      3.000 to 7.350, and the track scrolled. A matched control without input was
      identical at the start line and differed by 37,940 pixels after the input
      window. An exact replay matched both checkpoints, with zero exhausted
      frames. The title artwork appears heavily dithered; complete races, the
      track editor, saves, audio and browser performance remain unverified.
    status: playable
    evidence: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/dirt-bike-30.hqx
runtime:
  executable_path: Dirt Bike 3.0/Dirt Bike 3.0
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/dirt-bike-30.hqx
    expected_sha256: 1684c4c76f23bdeff4c7a4488f1b152533aa6cef434a4bde6bb4ec797233a3cf
    expected_size: 1315374
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/dirt-bike-30.hqx
    license: Original unregistered shareware distribution notice
    rights_holder: Brad Quick Software
    permission: >-
      The application's registration screen states that Dirt Bike may be
      distributed freely only in its unregistered form, and the program text asks
      users to distribute the program freely. Preserve the complete unchanged
      unregistered archive and documentation.
    notes: >-
      US$15 shareware. The archive contains the 68K application and five
      documentation files. Part of the code is licensed from Nisus Software Inc.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/dirt-bike/ride.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/dirt-bike-30.hqx
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: >-
      Native 68K Standard Track race after the rider leaves the start line,
      shown with the game's own menu bar.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/dirt-bike-30.hqx
---

## Ride the track

![Dirt Bike rider on the Standard Track](incoming/dirt-bike/ride.png)

Shift gears and balance your motocross bike over hills and jumps in this
physics-based racer. Race recorded opponents for the best time, or design your
own tracks and bikes.

Click **Not Yet** on the registration notice, then click to reach the start
line. Throttle depends on where the mouse is: at the top of the screen it is full
on, and at the bottom the brakes are full on. Press X or the mouse button to
shift up, and Z to shift down.
