---
id: spaceward-ho
kind: game
title: Spaceward Ho!
summary: >-
  Explore, settle and contest a galaxy in Delta Tao's original Macintosh strategy
  demo.
developer: Delta Tao Software
publisher: Delta Tao Software
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.57.0 + local site build"
    architecture: 68k
    environment: >-
      Release-mode browser launch from the immutable hosted 3.0 demo archive at
      localhost:8080; created a galaxy, joined as a novice player, and selected
      multiple planets on the interactive map.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2646
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: 0.52.0-dev
    architecture: 68k
    environment: >-
      Deterministic headless play from the unchanged 3.0 demo archive, through galaxy
      creation and player setup to the live map; startup compared with BasiliskII
      System 8.1.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2470
  - date: "2026-11-06"
    tester: Catalogue maintainer
    systemless_version: 73f1967d native ci-test
    architecture: 68k
    environment: >-
      The exact original demo displays its free-distribution notice and reaches
      the default galaxy through New Game, save-name acceptance and novice Join.
      Command-T advances to Year 2010; reserve metal changes from 20000 to 21065,
      ship savings from 0 to 33150 and total money from 51000 to 53612. A fresh
      repeat passes eight measured economy pixels; a matched no-input control
      passes four unchanged-economy pixels. Both end at 1059 frontend / 1658 guest
      ticks with zero exhausted frames. Independent extraction matches both files,
      all four forks and recovered application Finder metadata against production
      packing. The application has 25 CODE resources, no cfrg and no data fork.
      Full matches, colonisation, combat, saves, audio fidelity and independent
      hardware parity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5636
  - date: "2026-11-06"
    tester: Catalogue maintainer
    systemless_version: 0.110.1 public production
    architecture: 68k
    environment: >-
      Direct public Chrome 156 worker/WebGL input creates the default galaxy,
      joins as a novice and uses Command-T to reach the inspected Year 2010 report
      and changed economy. No frontend or archive substitution is used; the actual
      browser archive response matches its pinned hash and 401674-byte size.
      However the unchanged 20-second post-turn pacing check fails at a final
      steady snapshot of 24.612 guest ticks/s, below the 50 minimum, despite
      60.505 host FPS, maximum sampled worker frame 31.7 ms, minimum audio queue
      125.6 ms and no captured console errors. Launch is held pending repair and
      successful public requalification. This run does not establish qualified-live
      status, a complete game, save support or audio fidelity.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5637
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: aa58b78cb31e7be671ec84a4cc38bc3adc4d553a02adf20b6935e6bcee3c9a85
    size_bytes: 401674
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/spaceward-ho
    - https://groups.google.com/g/comp.sys.mac.games/c/-B3pUZJLoGg
    - https://www.ho.deltatao.com/
    - https://apps.apple.com/us/app/spaceward-ho/id510527152
    license: Delta Tao Software 3.0 demo redistribution permission
    rights_holder: Delta Tao Software
    permission: >-
      The original demo's introductory dialog explicitly permits free distribution of
      this demo without charge. This applies to the unchanged demo package, not any
      complete commercial edition.
    notes: >-
      Unchanged 401,674-byte StuffIt 3.0 demo archive, SHA-256
      aa58b78cb31e7be671ec84a4cc38bc3adc4d553a02adf20b6935e6bcee3c9a85. A contemporary FAQ documents the
      public demo. Delta Tao still identifies Spaceward Ho! as its product, while a
      separately developed iOS adaptation credited to Delta Tao and Ariton is currently sold;
      neither that adaptation nor a complete classic edition is hosted here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 775fc77bcf7db26e728893620aee2eb3bd569dd3a45c43d3ac83dfd4fbb7255c
    size_bytes: 30856
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2470
    permission: >-
      Original gameplay capture made for this catalogue at the maintainer's request.
      Underlying game artwork remains Delta Tao Software's property.
    notes: >-
      Deterministic capture from the exact unchanged 3.0 demo archive after joining a
      new galaxy. The 500-by-320 game-map surface was captured directly from the
      framebuffer at (300,74), excluding the Mac menu bar, desktop and window chrome without
      altering game pixels. PNG SHA-256
      775fc77bcf7db26e728893620aee2eb3bd569dd3a45c43d3ac83dfd4fbb7255c, 30,856 bytes.
references:
- https://www.ho.deltatao.com/
- https://apps.apple.com/us/app/spaceward-ho/id510527152
- https://groups.google.com/g/comp.sys.mac.games/c/-B3pUZJLoGg
- https://macintoshgarden.org/games/spaceward-ho
---

## Settle the stars

![Spaceward Ho! galaxy gameplay](https://assets.systemless.org/catalogue/media/sha256/77/775fc77bcf7db26e728893620aee2eb3bd569dd3a45c43d3ac83dfd4fbb7255c.png)

Spaceward Ho! begins by letting you shape the galaxy: choose its density and
layout, set the computer opponents' strength and decide how many will share the
map. Once inside, each named planet is a possible colony, resource source or
frontier. Your home world supplies the first ships and budget, while the map
quickly fills with decisions about exploration, technology and defence.

## The original 3.0 demo

This is Delta Tao's unchanged Macintosh demonstration, not the complete game.
Its own opening screen permits the demo to be given away without charge. The
catalogue preserves that exact package; the later commercial editions,
including the currently sold iOS adaptation, are not included.
