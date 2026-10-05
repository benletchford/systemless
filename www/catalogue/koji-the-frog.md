---
id: koji-the-frog
kind: game
launch_enabled: true
title: Koji the Frog 2.0.1
summary: Hop between lily pads, catch insects, and dodge hazards as Koji.
developer: Slimyfrog Software
publisher: Slimyfrog Software
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-10-04"
    tester: Catalogue maintainer
    systemless_version: 0.75.0 + deterministic play runner and 0.76.1 release browser build
    architecture: 68k
    environment: >-
      The unchanged 2.0.1 StuffIt package reaches its shareware notice, title screen,
      and level-one lily-pad arena in headless Systemless. Matched replays at tick
      1642 show Koji moving left when J is held, while the idle replay leaves him near the
      centre. The 0.76.1 release browser build fetched the same archive once, reached
      level one through Later and Play, and showed J moving Koji left. Two browser runs
      each had one 82-86 ms frame at the shareware notice; 99% of sampled frames were
      below 7 ms. Level completion and save behaviour remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4012
  - date: "2026-10-05"
    tester: Catalogue maintainer
    systemless_version: Browser build from dev/4038-koji-startup
    architecture: 68k
    environment: >-
      The exact immutable 2.0.1 archive loaded in a production-style browser
      preview. Later dismissed the shareware notice, Play entered the level-one
      arena, and holding J moved Koji left. Three focused 20-second startup
      probes with bounded 68k CPU slices had maximum runtime frames below 35 ms
      with advancing guest ticks and no browser console errors. Level completion
      and save behaviour remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4038
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: c2bec83fb36c1804707699333f7ec14a619a4f74020ee9bf25fbbee885d36a74
    size_bytes: 828251
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.macintoshrepository.org/3595-koji-the-frog
    rights_holder: Marco Carra and Slimyfrog Software
    permission: >-
      The bundled Software License permits non-profit distribution of the complete,
      unaltered work. This entry keeps the original shareware archive intact for free
      online play.
    notes: >-
      Original 828,251-byte StuffIt archive, SHA-256
      c2bec83fb36c1804707699333f7ec14a619a4f74020ee9bf25fbbee885d36a74. The linked catalogue page documents the title;
      its downloadable package differs from this exact 2.0.1 archive.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 55936602db0805d05787162425f94884e0f69083794b53832a691f31ff01d9b4
    size_bytes: 19567
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4012
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only release browser capture of the level-one lily-pad arena, excluding
      browser and emulator framing. SHA-256
      55936602db0805d05787162425f94884e0f69083794b53832a691f31ff01d9b4.
references:
- https://www.macintoshrepository.org/3595-koji-the-frog
---

## Leap into the lily-pad arena

![Koji in the level-one lily-pad arena](https://assets.systemless.org/catalogue/media/sha256/55/55936602db0805d05787162425f94884e0f69083794b53832a691f31ff01d9b4.png)

Dismiss the shareware notice with **Later**, then choose **Play** on the title
screen. The in-game Controls screen maps **J** and **L** to moving left and
right, **U**, **I**, and **O** to jumping, and **Space** to Koji's tongue.

This original shareware package limits play and some features until registration.
