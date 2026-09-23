---
id: shanghai-ii-dragons-eye
kind: game
title: "Shanghai II: Dragon's Eye"
summary: >-
  Match pairs of tiles in Brodie Lockard's original Macintosh demonstration of
  Shanghai II.
developer: Brodie Lockard
publisher: Activision
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
      Deterministic headless run of the unchanged Macintosh demo archive. The
      Shanghai II Demo application loaded a fully drawn tile board at 800-by-600; browser
      interaction has not yet been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2497
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 6ffb97e2f4a4621a27d3246892fc70612ee27523094d4fe4cca875f6a2f8bb7e
    size_bytes: 402023
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/shanghai-ii-dragons-eye
    - https://static.classicmacdemos.com/demos/shanghai-ii-dragons-eye/README.txt
    rights_holder: Shanghai II rights holders
    permission: >-
      This unchanged promotional demo was distributed to introduce Shanghai II. Its
      included read-me, written by game author Brodie Lockard, explicitly calls it a
      demo and describes the retail game's additional art, sounds, layouts, variations and
      construction tools. Classic Macintosh Game Demos preserves the demo and
      documents contemporary demo-disc distribution. The archive does not contain the retail
      game and has no express redistribution clause.
    notes: >-
      Original 402,023-byte StuffIt archive containing Shanghai II Demo and its
      read-me. SHA-256 6ffb97e2f4a4621a27d3246892fc70612ee27523094d4fe4cca875f6a2f8bb7e.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a3261164ee8883be9fcc5b81e8d057c60cd3c0cca2c675938766d123cc96478a
    size_bytes: 139820
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2497
    permission: >-
      Fresh deterministic game-board capture made for this catalogue entry.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Captured from the same unchanged archive after the demo reached the Shanghai
      tile board. The deterministic capture selected a 643-by-460 game-content rectangle
      at (2,42), excluding the Classic Mac menu bar and desktop margins without
      altering game pixels. PNG SHA-256
      a3261164ee8883be9fcc5b81e8d057c60cd3c0cca2c675938766d123cc96478a, 139,820 bytes.
references:
- https://classicmacdemos.com/shanghai-ii-dragons-eye
- https://static.classicmacdemos.com/demos/shanghai-ii-dragons-eye/README.txt
---

## Clear the tile board

![Shanghai II demo tile board](https://assets.systemless.org/catalogue/media/sha256/a3/a3261164ee8883be9fcc5b81e8d057c60cd3c0cca2c675938766d123cc96478a.png)

Shanghai II builds on the original game's tile-matching puzzle: find exposed
pairs, clear them, and try to remove the entire pile before you run out of
moves. Its Macintosh demo presents the game's colourful tiles and full board.

This is the original promotional demo, not the commercial game. The included
read-me by author Brodie Lockard identifies it as a demo and describes the
additional layouts, tile sets, music, variations and editing tools offered by
the full release. Systemless loads the original 68K application and renders
the tile board; browser play awaits manual approval.
