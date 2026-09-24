---
id: welltris
kind: game
title: Welltris
summary: >-
  Guide falling shapes down a three-dimensional well in Spectrum HoloByte's
  Macintosh demo.
developer: Sphere
publisher: Spectrum HoloByte
year: 1990
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.56.0 development build with PR #2627"
    architecture: 68k
    environment: "Deterministic run of the unchanged Macintosh promotional demo through its title, setup screen, and live 3D well. Level, speed, score, and lives values match a BasiliskII run after the generic monochrome foreground correction in PR #2627. A local Chromium browser route also reached title, setup, and live play with the promoted immutable archive."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2501
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 89fa666641c00e0484241f6be734a3078c684dcc4a37ccca2133184778214153
    size_bytes: 111943
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/welltris
    - https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
    rights_holder: Welltris rights holders
    permission: >-
      This is Spectrum HoloByte's limited Macintosh promotional demo, independently
      present on Apple's 1992 Macintosh Demo Games CD. The archive contains the demo
      application, not the commercial game. No express redistribution licence was found;
      this preservation basis does not extend to retail media or repacks.
    notes: >-
      The unchanged 111,943-byte StuffIt archive has SHA-256
      89fa666641c00e0484241f6be734a3078c684dcc4a37ccca2133184778214153.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 5baf81196052fdd82cb34e270379c8cfbd69164b3a1d6365842809d4e96a1555
    size_bytes: 41080
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2501
    permission: >-
      Fresh gameplay screenshot made for this catalogue from the unchanged demo.
      Underlying Welltris artwork remains the property of its rights holders.
    notes: >-
      Captured in Systemless after Start Game, then cropped to the 512-by-342
      game-content rectangle at (144,130), excluding the Classic Mac menu bar, window frame,
      and desktop without altering game pixels. PNG SHA-256
      5baf81196052fdd82cb34e270379c8cfbd69164b3a1d6365842809d4e96a1555; 41,080 bytes.
references:
- https://classicmacdemos.com/welltris
- https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
- https://github.com/benletchford/systemless/issues/2501
- https://github.com/benletchford/systemless/issues/2625
---

## A new angle on falling blocks

![Welltris demo during live play](https://assets.systemless.org/catalogue/media/sha256/5b/5baf81196052fdd82cb34e270379c8cfbd69164b3a1d6365842809d4e96a1555.png)

*Welltris* sends each falling shape down one of four walls toward the floor of a
square well. Rotate and position the pieces to complete lines while the board
becomes faster and more demanding.

This entry preserves the original Macintosh promotional demo, not the retail
game. Click the title screen, then select **Start Game** to reach the well.
