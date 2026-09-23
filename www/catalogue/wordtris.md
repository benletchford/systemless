---
id: wordtris
kind: game
title: Wordtris
summary: >-
  Form words from falling letter tiles in Spectrum HoloByte's original Macintosh
  demo.
developer: Spectrum HoloByte
publisher: Spectrum HoloByte
year: 1991
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.53.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged Macintosh demo archive. The game
      passed its opening screens and reached a live falling-letter board after two Return
      presses. Browser interaction has not yet been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2502
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 02a41e753bae58b1dc7861a7083b3c16a3e34ff49df902820a3c941042e3e498
    size_bytes: 425113
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/wordtris
    rights_holder: Wordtris rights holders
    permission: >-
      This unchanged promotional Macintosh demo was distributed on contemporary demo
      discs. The archive contains the demo application and its accompanying resources,
      not the commercial game; it has no express redistribution clause.
    notes: >-
      Original 425,113-byte StuffIt archive, SHA-256
      02a41e753bae58b1dc7861a7083b3c16a3e34ff49df902820a3c941042e3e498.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 1f483a804a38b7c6ecd9c2c2626423fb94b4adef788fe10904fb305dbc8c6a7b
    size_bytes: 254539
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2502
    permission: >-
      Fresh deterministic game-board capture made for this catalogue entry.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Captured directly from the unchanged demo at the live falling-letter board. The
      515-by-324 game-content rectangle at (143,149) excludes the Classic Mac menu bar
      and desktop without altering game pixels. PNG SHA-256
      1f483a804a38b7c6ecd9c2c2626423fb94b4adef788fe10904fb305dbc8c6a7b, 254,539 bytes.
references:
- https://classicmacdemos.com/wordtris
---

## Falling letters, forming words

![Wordtris Macintosh demo game board](https://assets.systemless.org/catalogue/media/sha256/1f/1f483a804a38b7c6ecd9c2c2626423fb94b4adef788fe10904fb305dbc8c6a7b.png)

Wordtris combines a falling-block game with word puzzles. Position each letter
as it descends, then spell words across the board to score points. This
Macintosh demo shows the illustrated opening screens and a playable letter well.

This entry uses the original promotional demo, not the commercial release.
Systemless reaches live play with the 68K application; browser launch awaits
manual approval.
