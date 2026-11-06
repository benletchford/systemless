---
id: continuum
launch_enabled: true
kind: game
title: Continuum 1.03
summary: >-
  Pilot a thrust-and-gravity ship through cavernous planets, shooting bunkers and
  collecting fuel in the Wilson brothers' classic black-and-white Mac shooter.
developer: Randall and Brian Wilson
year: 1990
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 0fdb3961 deterministic play runner
    architecture: 68k
    environment: >-
      The original funet archive ran natively in headless mode after the MacBinary
      padding repair in 0fdb3961. Game > Start Game opened Planet 1. Holding the default
      thrust key (/) for 60 ticks moved the ship and burned fuel. A matched control
      without the key was identical until the input and then differed by 894 pixels. An
      exact replay matched both checkpoints, with zero exhausted frames. Combat, shields,
      later planets and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5453
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: e17408c0c native and release browser recheck
    architecture: 68k
    environment: >-
      Native Planet 1 thrust passes six measured pixel assertions and three
      exact replay-image checks, with zero exhausted frames. A matched no-thrust
      run is identical before input and differs by 929 pixels afterwards. Chrome
      156 worker/WebGL previews use local frontend delivery at the production
      origin and fetch the unchanged managed original directly. Held menu input
      starts Planet 1; slash thrust visibly moves the ship and reduces fuel,
      while a same-duration control retains 10,000 fuel. Separate 20-second
      thrust/control samples pass unchanged pacing thresholds at final snapshots
      of 60.935 / 58.460 guest ticks/sec and 59.920 / 62.712 host FPS, maximum
      sampled worker frames 11.6 / 11.7 ms and minimum audio queues 84.6 / 84.8 ms,
      with no console errors. Combat, shields, level completion, later planets,
      saves, audio fidelity and unmodified public deployment remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5453
runtime:
  executable_path: Continuum
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: c4e555fe3f3ec5d0e1ee5c05fedbc093c2b38408ea696bb6916f13fdfefdcf02
    size_bytes: 137613
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/games/continuum103.sit
    - https://github.com/benletchford/systemless/issues/5453
    license: Original Beerware distribution notice
    rights_holder: Randall H. Wilson
    permission: "The application's notice says Continuum is distributed under the Beerware system: you may copy it freely and give it to anyone, as long as it is not modified or sold. Preserve the complete unchanged archive."
    notes: >-
      MacBinary-wrapped StuffIt archive containing the 68K application, the default
      galaxy and the planet editor documentation.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 11d12d7df8d9dc5eebace48819f29a35870d1fb74254f218a8eb41aee7a9f1d6
    size_bytes: 3231
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5453
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K Planet 1 after thrusting, cropped to the game window contents.
references:
- https://github.com/benletchford/systemless/issues/5453
---

## Fly the planets

![Continuum Planet 1](https://assets.systemless.org/catalogue/media/sha256/11/11d12d7df8d9dc5eebace48819f29a35870d1fb74254f218a8eb41aee7a9f1d6.png)

Steer your ship through each planet's gravity, destroy the bunkers and use your
shield to pick up fuel before you run dry. The game includes a planet editor for
building your own galaxies.

Choose **Game > Start Game**. By default, Option turns left, / thrusts, Shift
fires and Space raises the shield. Change the keys with **Game > Set Controls**.
