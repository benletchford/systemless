---
id: super-tetris
kind: game
title: Super Tetris
summary: Drop and clear colourful blocks in Spectrum HoloByte's original Macintosh demo.
developer: Spectrum HoloByte
publisher: Spectrum HoloByte
year: 1991
architectures: [68k]
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.53.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged Macintosh demo archive.
      The colour application passed its title and instruction screens and
      reached the live falling-block board after two Return presses.
      Browser interaction has not yet been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2499
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Super%20Tetris.sit
    expected_sha256: 9364e253605e3b06a498453ab0d389731bd21bca001e260c47da5549f8858e6f
    expected_size: 591262
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/super-tetris
    - https://www.macintoshrepository.org/4323-super-tetris
    rights_holder: Super Tetris rights holders
    permission: >-
      This unchanged promotional archive contains two purpose-built Macintosh
      demo applications, one monochrome and one colour, plus their sound file.
      Classic Macintosh Game Demos documents its distribution on multiple
      contemporary demo and public-domain discs. Only the demo package is
      hosted, not a retail disk; the package has no express redistribution clause.
    notes: >-
      Original 591,262-byte StuffIt archive, SHA-256
      9364e253605e3b06a498453ab0d389731bd21bca001e260c47da5549f8858e6f.
      The colour demo application was used for compatibility verification.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/super-tetris/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2499
    permission: >-
      Fresh deterministic game-board capture made for this catalogue entry.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Captured directly from the unchanged colour demo after starting a game.
      The capture selected the 515-by-323 game-content rectangle at (143,150),
      excluding the Classic Mac menu bar and desktop without altering game pixels.
      PNG SHA-256
      f5f556b55e094507f73b7de8a485df99c60f048c202114f6b3a3bb39031cf600,
      250,446 bytes.
references:
- https://classicmacdemos.com/super-tetris
- https://www.macintoshrepository.org/4323-super-tetris
---

## Falling blocks with a twist

![Super Tetris colour demo game board](incoming/super-tetris/gameplay.png)

Super Tetris keeps the familiar rhythm of moving and rotating falling pieces,
then adds treasure blocks and bombs to the board. The Macintosh colour demo
opens with illustrated instructions before letting the player start a game.

This entry preserves Spectrum HoloByte's original promotional demo archive,
not the commercial release. It contains separate black-and-white and colour
applications; Systemless has been checked with the colour version. Browser
launch awaits manual approval.
