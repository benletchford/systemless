---
id: mario-teaches-typing-demo
launch_enabled: true
kind: game
title: Mario Teaches Typing Demo
summary: >-
  Learn to touch-type with Mario, hopping over letter-marked turtles and blocks
  as you hit the right keys in Interplay's typing tutor.
developer: Interplay Productions
publisher: Interplay Productions
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: "The unchanged demo archive ran natively on the 68K slice and reached the lesson menu. Clicking lesson 1 opened the typing game with Mario and the letter turtles. A matched idle control was identical until the input and then differed by 160,938 pixels. An exact replay matched both checkpoints, with zero exhausted frames. Typing scoring, other lessons, and audio remain unverified. Browser pacing during the lesson was inconsistent: about 17–20 guest ticks/s at default pacing and 27–60 at 6 MHz, so launch stays disabled."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5510
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: e17408c0c native and release browser recheck
    architecture: 68k
    environment: >-
      Fresh native lesson 1 accepts its displayed F, showing Typed 1 and Errors 0.
      Six measured pixel assertions and two exact replay-image checks pass with
      zero exhausted frames; the matched no-key control differs by 11,173 pixels
      only after input. Chrome 156 worker/WebGL previews fetch the unchanged
      managed archive directly with local frontend delivery at the production
      origin. At the new 10 MHz per-entry target, typing the displayed J produces
      results with Typed 1, Errors 0 and Accuracy 100%. Separate 20-second scored
      lesson/results and results-idle samples pass unchanged thresholds: final
      guest snapshots 59.302 / 60.090 ticks/sec, host 61.383 / 62.199 FPS, maximum
      sampled worker frames 17.0 / 13.9 ms, minimum audio queues 348.7 / 422.3 ms,
      without console errors. The former 25 MHz results-idle run failed at 31.649
      ticks/sec; menu-only probes were excluded. Other lessons, sustained typing,
      saves, audio fidelity, reference timing parity and unmodified production
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5510
runtime:
  show_menu_bar: true
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 7be55665587319638f496c3a0f171964117842d625abc373e80ba1917c096d4c
    size_bytes: 1145949
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/mario-teaches-typing
    - https://download.classicmacdemos.com/Mario%20Demo%20folder.sit
    - https://github.com/benletchford/systemless/issues/5510
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Interplay Productions and Nintendo
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: 68K demo of Interplay's typing tutor starring Mario.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 33358edac43c0ae39ca0c7ffae8d79cfc540592beef2bbf6c1b8bcc9c6c346c7
    size_bytes: 35561
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5510
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: Native 68K lesson 1 typing game, cropped to the game window.
references:
- https://classicmacdemos.com/mario-teaches-typing
- https://github.com/benletchford/systemless/issues/5510
---

## Type to jump

![Mario Teaches Typing Demo](https://assets.systemless.org/catalogue/media/sha256/33/33358edac43c0ae39ca0c7ffae8d79cfc540592beef2bbf6c1b8bcc9c6c346c7.png)

Each lesson turns typing drills into a short Mario run. Hit the letter shown on each turtle or block to keep Mario moving, while the game tracks your speed and errors.

Pick a lesson from the menu, then type the letters shown on screen.
