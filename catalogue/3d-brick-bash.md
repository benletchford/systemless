---
id: 3d-brick-bash
kind: game
title: 3D Brick Bash!
summary: >-
  Break through layers of bricks in a pseudo-3D wireframe arena, keeping the ball
  in play with a mouse-driven paddle.
developer: Matthew Diamond
publisher: Matthew Diamond
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-18
    tester: Catalogue maintainer
    systemless_version: 0.41.9
    architecture: 68k
    environment: Deterministic headless run from the Info-Mac mirror distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/223
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: b660171f21bcf0e12a8f23d08f74246453cab3894b8f443b05be40faf98a5143
    size_bytes: 151357
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/3d-brick-bash-14-68k.hqx
    license: Freeware
    rights_holder: Matthew Diamond
    permission: "The included distribution notice explicitly states: \"This release removes shareware notices, making it a freeware game. Replaces earlier versions, such as 3dBrick.* and 3dBrickBash1.3.1.sit.hqx. O.K. to include on Info-Mac CD archives. Matt Diamond\""
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 151,357-byte file has SHA-256
      b660171f21bcf0e12a8f23d08f74246453cab3894b8f443b05be40faf98a5143. The original author retains copyright and has no active
      modern commercial releases or storefront listings for this title.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 989e5577bc47d608f2c56f48f2da146374726ac759153f7140c46d7e1ee07e38
    size_bytes: 17193
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/223
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-18, cropped exactly to the game's 519-by-335 content surface. It excludes the
      Classic Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/3d-brick-bash-14-68k.hqx
---

## Deep wireframe breakout action

![3D Brick Bash! gameplay](https://assets.systemless.org/catalogue/media/sha256/98/989e5577bc47d608f2c56f48f2da146374726ac759153f7140c46d7e1ee07e38.png)

Released in 1993 by Matthew Diamond, *3D Brick Bash!* takes the classic paddle-and-ball breakout formula into a pseudo-three-dimensional wireframe corridor. Rather than deflecting a ball along a two-dimensional vertical plane, players maneuver a rectangular paddle across the near viewport, projecting the ball forward into the depth of the tunnel towards tiered walls of colored bricks.

As the ball bounces against side walls, ceiling, floor, and target bricks, depth scaling and perspective calculations simulate physical trajectory into the screen. Accurate paddle positioning and spin control are required to clear layered layouts while preventing the ball from flying past the player's perimeter.

## Native execution

Systemless executes *3D Brick Bash!* within its 68K runtime, providing standard Window Manager window structures, Color QuickDraw graphics operations, mouse tracking, and low-latency frame timing directly in the browser without requiring a full Macintosh operating system install.
