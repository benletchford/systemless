---
id: snake-byte
kind: game
title: Snake Byte 1.2
summary: >-
  Steer a growing snake around a grassy field, eating apples and dodging rocks,
  in this freeware classic.
developer: PLP
year: 1995
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
    systemless_version: cc649233 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. After the welcome
      dialog, File > New Game started level 1. The default relative-turn keys are
      keypad 4 and 6. A single keypad 4 press turned the snake left; the matched
      control without the key was identical until the input and then differed by
      1,683 pixels (keypad 6: 1,674). Arrow and letter keys had no effect, so the
      runtime maps arrows to the keypad. An exact replay matched both checkpoints,
      with zero exhausted frames. Apples, rocks, audio and longer play remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5423
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: cc649233 browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of
      a locally served trunk build used the exact pinned original archive. Scripted
      ordinary input dismissed the welcome window, started a game with Command-N and
      pressed Left. Each sample reached live play. Twenty-second samples reached
      60.07, 60.00 and 59.98 guest ticks per second, with no console errors. This
      approves bounded browser launch; the browser arrow-to-keypad turn was not
      separately measured, and deployed replay, full levels and audio remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5423
runtime:
  executable_path: Snake Byte
  show_menu_bar: true
controls:
  arrows_as_numpad: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/snake-byte-12.hqx
    expected_sha256: d567078dfd9d8a85ecab5495a9e4714e6b47192b4c41b683088caab787f93dcd
    expected_size: 649131
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/arc/snake-byte-12.hqx
    - https://github.com/benletchford/systemless/issues/5423
    license: Original freeware distribution notice
    rights_holder: PLP
    permission: >-
      The application's FreeWare Notice permits distributing Snake Byte as long as
      it is distributed free of charge and unmodified. Preserve the complete
      unchanged archive.
    notes: The archive contains only the 68K application; its documentation is built in.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/snake-byte/field.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5423
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K level 1 after the snake turns left, cropped to the game window contents.
references:
- https://github.com/benletchford/systemless/issues/5423
---

## Eat your apples

![Snake Byte level 1](incoming/snake-byte/field.png)

Guide your snake to eat apples. Each one makes it longer and faster. Eat enough
apples to open the way to the next level, and avoid walls, rocks and your own
tail.

Click **OK** on the welcome window, then choose **File > New Game**. The left and
right arrow keys turn the snake relative to its heading. Click to pause.
