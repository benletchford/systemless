---
id: tomb-raider-gold-demo
kind: game
title: Tomb Raider Gold Demo
summary: >-
  Explore part of Tomb Raider's second level, the Lost Valley approach, in the
  original Power Macintosh demo.
developer: Westlake Interactive
publisher: Aspyr Media
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 0.102.0 deterministic play runner at d71b3b09
    architecture: ppc
    environment: >-
      The original archive ran natively in headless mode. After the first-run
      hardware configuration notice, the title ring appears. Return selects
      Game and New Game, which reaches the live 3D level by frontend tick 1629.
      Holding Up for 180 ticks runs Lara out of the opening corridor into the
      next chamber. A matched control script without the Up key is identical
      at every earlier checkpoint. After the input window, 298,131 of 307,200
      pixels differ. An exact replay matches all three checkpoints, with zero
      exhausted frames. Browser performance, combat, saves, audio and longer play
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4181
runtime:
  executable_path: Tomb Raider Gold/Tomb Raider I Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Tomb%20Raider%20Gold.sit
    expected_sha256: b7a5fd3e3e2f529bc8e7a4fce0c65d54033638e7baf104d14debf24da664d4c0
    expected_size: 2724762
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/tomb-raider-1-gold
    - https://www.application-systems.de/tombraider/tr1/
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Aspyr Media; Core Design / Eidos
    permission: >-
      Hosting was accepted by the Systemless maintainer on 31 October 2026. The
      unchanged demo was distributed free of charge on at least sixteen period
      Macintosh magazine cover discs and publisher download pages. The bundled
      Read Me contains no explicit redistribution grant or restriction. Preserve
      the complete unchanged archive, Read Me and ordering information, and
      remove it if a rights holder objects.
    notes: >-
      The archive contains nine files: the PowerPC demo, setup application,
      Game Sprockets installer, Read Me and four level and title data files. The
      demo includes portions of Tomb Raider I's second level.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/tomb-raider-gold-demo/chamber.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/pull/4181
    permission: >-
      Fresh content-only gameplay capture from the unchanged original demo
      archive. The underlying game artwork remains its owners' property.
    notes: >-
      Native PowerPC frame after Lara runs from the opening corridor into the next
      chamber; excludes the desktop and emulator interface.
references:
- https://classicmacdemos.com/tomb-raider-1-gold
- https://www.application-systems.de/tombraider/tr1/
---

## Enter the lost ruins

![Lara in a stone chamber in the Tomb Raider Gold demo](incoming/tomb-raider-gold-demo/chamber.png)

Guide Lara Croft through caves and ruins in the original Power Macintosh demo of
Tomb Raider Gold. The demo includes part of the second level of Tomb Raider I.

Click **OK** on the first-run hardware notice. Press Return to choose **Game**,
then Return again for **New Game**. Use the arrow keys to run and turn, Command to
jump, Control for actions, Shift to walk and Space to draw weapons. Press Escape
for the inventory ring.
