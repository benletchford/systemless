---
id: avernum-demo
kind: game
title: Avernum Demo
summary: >-
  Lead four exiles through an underground world in Spiderweb Software's original
  fantasy role-playing demo.
developer: Jeff Vogel
publisher: Spiderweb Software
year: 2000
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
compatibility:
  status: unknown
  verified:
  - date: "2026-11-09"
    tester: Catalogue maintainer
    systemless_version: 505571c3 native candidate; browser qualification pending
    architecture: ppc
    environment: >-
      Complete unchanged Avernum v1.0.2 demo installer; exact executable Avernum
      v1.0.2 Demo Installer/Avernum ƒ/Avernum v1.0.2. Default PPC 25 MHz, original display
      settings and preferred 12,288,000-byte partition. Choose Switch Resolution This
      Time, dismiss the intro and Welcome, choose New Game, retain the default
      four-character party, continue the story and dismiss Instant Help. At Fort Avernum,
      ordinary keypad 6 moves the party and scrolls the map; equal-clock omitted-key control
      retains its position. Fresh active/control replays each pass six measured scene
      checks at 2,173 frontend and guest ticks, with zero exhausted frames. Actual
      captures inspected; own captures establish consistency, not a hardware oracle. Native
      binary SHA-256 9df329380117e0adf06b5c582483febefc7aa1fdfe67f771222be1fa574832ba.
      Requires the resource-file working-directory fix in PR 5874. A small
      dismissed-help-button remnant remains visible. Browser input, rendering and pacing, combat,
      complete quests, persistence and audio fidelity remain unverified. No
      registration, date, game-byte or partition changes.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5875
  - date: "2026-11-09"
    tester: Catalogue maintainer
    systemless_version: 4cee18e7 private browser preview on merged runtime 52b75be6
    architecture: ppc
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5877
    environment: >-
      Hosted unchanged original archive and exact main/worker Wasm response bodies
      verified by HTTP 200, size and SHA-256. Private source differs from promoted
      ef7295dd only by a test launch flag; runtime Rust matches merged 52b75be6.
      Worker/WebGL, isolation and SharedArrayBuffer, default PPC 25 MHz, original
      800x600/16-bit display, preferred partition, default party and sound retained.
      Each unchanged 20-second sample begins at the final story before Fort Avernum
      initialization, includes help dismissal, then sends keypad 6 or omits it.
      Both runs pass six measured scene checks; actual captures inspected. The
      native 800x600 framebuffer renders at CSS 798x598, and exact RGB samples use
      native-coordinate projection with a one-CSS-pixel neighborhood. The active
      party/map moves while the control retains its position. Both fail pacing:
      final steady guest rates 25.7923/24.3471 ticks per second and host rates
      50.7248/49.5338 FPS, maximum worker frames 166.8/304.0 ms and minimum audio
      queues 11.8/13.5 ms. Gates remain guest >=50, host >=55, frame <=50 ms and
      audio >=40 ms. No concurrent build or captured lifecycle/network errors.
      Actual archive is 7,385,878 bytes with SHA-256
      1b9009fab8364fb952cab13ebb6aeabf94702d619c77bd6872ba347fd1c7f89e.
      Both Wasm bodies are 17,970,823 bytes with SHA-256
      5591909f2f28a138ea5a6070a431cd4e575224dc50131dc9ce7810d769678b1d.
      Earlier attempts stopped during harness setup before any pacing sample.
      A small dismissed-help-button remnant remains visible. Browser launch is
      unapproved; full quests, combat, saves and audio fidelity remain unverified.
runtime:
  executable_path: Avernum v1.0.2 Demo Installer/Avernum ƒ/Avernum v1.0.2
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 1b9009fab8364fb952cab13ebb6aeabf94702d619c77bd6872ba347fd1c7f89e
    size_bytes: 7385878
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/a/
    - https://github.com/benletchford/systemless/issues/5875
    license: Complete bundled Spiderweb Software License
    rights_holder: Spiderweb Software, Inc.
    permission: >-
      Section 2 of the complete bundled licence permits non-profit distribution
      without prior written notice if the software is unmodified and the complete works are
      included. Preserve all files and notices, the unregistered demo and shareware
      state, and the original thirty-day licence condition. No paid distribution or
      registration, date or other restriction bypass.
    notes: >-
      Complete original 7,385,878-byte StuffIt demo installer. The full 7,737-byte
      licence and complete tech-support document were read. Production extraction retains
      fourteen paths, including the game, original art/sound and town data,
      instructions/order form, licence, support document, map, CD promotion and bundled
      DrawSprocketLib. Original outer archive bytes are preserved; no files removed, no
      repacking and no added runtime. Separate MacintoshRepository archive has a different hash
      and is not claimed byte-identical. This package is Avernum v1.0.2, not Exile, a
      later remake or an unlocked edition.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 07a29d98a77090c064c81ec398f8409fcaff7f37dad16487a2b6c24283a5d453
    size_bytes: 997597
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5875
    permission: >-
      Original Systemless gameplay capture of this unregistered demo; underlying
      artwork remains Spiderweb Software's property.
    notes: >-
      Unedited 800x600 game content capture after one keypad-6 movement in Fort
      Avernum, from the exact original archive on runtime 505571c3. Contains no desktop,
      Classic menu bar, browser or emulator controls. The small dismissed help-button
      remnant is preserved; no retouching or resampling.
references:
- https://www.vintageapplemac.com/software/games/a/
- https://github.com/benletchford/systemless/issues/5875
---

## Begin beneath the Empire

![The original Avernum demo party in Fort Avernum](https://assets.systemless.org/catalogue/media/sha256/07/07a29d98a77090c064c81ec398f8409fcaff7f37dad16487a2b6c24283a5d453.png)

Create a party of four adventurers and explore Avernum's underground towns and
caves. This is Spiderweb Software's complete original unregistered Macintosh
v1.0.2 demo, including its instructions, map and licence.

The default party reaches Fort Avernum in native testing. Use the numeric keypad
or click terrain to move. Browser launch is unavailable while performance is being improved.
