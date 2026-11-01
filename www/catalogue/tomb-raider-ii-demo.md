---
id: tomb-raider-ii-demo
kind: game
title: Tomb Raider II Demo
summary: >-
  Explore the Great Wall of China with Lara Croft in the Power Macintosh demo of
  Core Design's 1997 action-adventure sequel.
developer: Core Design; Westlake Interactive (Macintosh)
publisher: Aspyr Media
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 12dda612 deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged demo archive ran natively on the PowerPC slice. After the
      first-run hardware configuration notice, the demo opened directly in the Great Wall
      level with the software renderer. Holding Up for 120 ticks ran Lara forward into the
      canyon. A matched control without the key was identical until the input and then
      differed by 289,560 pixels. An exact replay matched both checkpoints, with zero
      exhausted frames. Combat, jumping, saves, audio and browser performance remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5479
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 12dda612 browser build with launch flag and 60 MHz adaptive pacing
    architecture: ppc
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a
      locally served trunk build used the exact pinned original archive with runtime
      pacing at 60 MHz (adaptive minimum 25 MHz). Scripted ordinary input dismissed the
      first-run notice and held Up in the Great Wall level. Twenty-second samples
      reached 58.32, 61.22 and 59.89 guest ticks per second (host 64.92, 63.37 and 61.99
      fps), with no console errors. This approves bounded browser launch; combat, saves
      and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5479
runtime:
  executable_path: Tomb Raider II/Tomb Raider II Demo
  runtime_pacing:
    adaptive_min_cpu_mhz: 25
    cpu_mhz: 60
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: c59230c28f7d8ba9934bc298f56cd4472dc9983ba816bf1b2590e217e9d3fc30
    size_bytes: 5896317
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/tomb-raider-2
    - https://download.classicmacdemos.com/Tomb%20Raider%20II.sit
    - https://github.com/benletchford/systemless/issues/5479
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Core Design / Eidos; Aspyr Media
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on at least 33 period Macintosh magazine cover discs. The bundled documents
      contain no explicit redistribution grant or restriction. Preserve the complete
      unchanged archive and remove it if a rights holder objects.
    notes: >-
      PowerPC demo with part of the Great Wall level, its setup application and the
      Game Sprockets and ATI installers.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 26dc379f74a19f688309ba390a3d25d803eadac3a90b87d5a2b29c1cbd483a4c
    size_bytes: 647318
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5479
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native PowerPC Great Wall canyon after Lara runs forward.
references:
- https://classicmacdemos.com/tomb-raider-2
- https://github.com/benletchford/systemless/issues/5479
---

## The Great Wall

![Tomb Raider II Demo](https://assets.systemless.org/catalogue/media/sha256/26/26dc379f74a19f688309ba390a3d25d803eadac3a90b87d5a2b29c1cbd483a4c.png)

Guide Lara Croft through caves and ruins along the Great Wall of China.

Click **OK** on the first-run notice. Use the arrow keys to run and turn, Command to jump, Control for actions and Shift to walk. Press Escape for the inventory ring.
