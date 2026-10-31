---
id: looney-lander
kind: game
title: Looney Lander 0.9
summary: >-
  Rotate and thrust your lunar module down to a landing pad before the fuel runs
  out in MASS Productions' colourful lander game.
developer: MASS Productions
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 05ea3765 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. After the title,
      File > New Game opened the lunar landscape with the lander and fuel gauge.
      Holding K (thrust) for 90 ticks burned fuel and moved the lander. A matched
      control without the key was identical until the input and then differed by
      1,032 pixels. An exact replay matched both checkpoints, with zero exhausted
      frames. The File menu title stays highlighted during play. Landings, scoring
      and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5427
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 05ea3765 browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of
      a locally served trunk build used the exact pinned original archive. Scripted
      ordinary input passed the title, started a game with Command-N and thrust with
      K. Each unattended run reached Game Over within the sample window. Twenty-
      second samples reached 58.98, 58.90 and 59.86 guest ticks per second (host
      61.05, 62.05 and 60.89 fps), with no console errors. This approves bounded
      browser launch; deployed replay, landings and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5427
runtime:
  executable_path: Looney Lander ƒ/Looney Lander
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/looney-lander.hqx
    expected_sha256: 7a2357cf7b1be3c8e1c71c833af42fb8e9af51b2160a046ea948d1b393ab198f
    expected_size: 383736
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/looney-lander.hqx
    - https://github.com/benletchford/systemless/issues/5427
    license: Original free distribution notice
    rights_holder: MASS Productions (Tom Stromar and Andrew Reimer)
    permission: >-
      The bundled Readme's distribution and licence agreement says Looney Lander
      may be freely distributed as long as the Readme accompanies it. Preserve the
      complete unchanged archive.
    notes: The archive contains the 68K application and Readme.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/looney-lander/landing.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5427
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K lunar landscape after thrusting, cropped to the game window contents.
references:
- https://github.com/benletchford/systemless/issues/5427
---

## Bring it down gently

![Looney Lander lunar landscape](incoming/looney-lander/landing.png)

Steer your lander between jagged peaks and touch down softly on the landing pad
before your fuel runs out. Higher gravity earns a bigger bonus.

Choose **File > New Game**. Press J or keypad 4 to rotate left, K or keypad 5 to
thrust, and press ? for help. Gravity and difficulty are set in the Options menu.
