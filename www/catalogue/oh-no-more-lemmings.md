---
id: oh-no-more-lemmings
kind: game
title: Oh No! More Lemmings Demo
summary: Rescue the crowd in Psygnosis's four-level Macintosh promotional mini-game.
developer: DMA Design
publisher: Psygnosis
year: 1993
architectures: [68k]
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: 2026-09-28
    tester: Catalogue maintainer
    systemless_version: 341e1c18dd0cf271b5f75a964206a123c0c89466
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged StuffIt demo. The introduction
      and menu opened, Level 1 Citizen Lemming started, a lemming moved, and the
      level timer advanced. Browser launch and native-Mac comparison have not yet
      been verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3067
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Oh%20No%20More%20Lemmings.sit
    expected_sha256: 64889d7c946395629f76fa38083d2d756e75ead5df233497ea9b12bcc6894640
    expected_size: 493822
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/oh-no-more-lemmings
    - https://download.classicmacdemos.com/Oh%20No%20More%20Lemmings.sit
    - https://static.classicmacdemos.com/demos/oh-no-more-lemmings/README.txt
    - https://www.playstation.com/en-gb/legal/copyright-and-trademark-notice/
    rights_holder: Sony Interactive Entertainment Europe Limited
    permission: >-
      Psygnosis released this self-contained package as The Demo Disk. The bundled
      Read Me and opening screens identify a four-level promotional mini-game,
      distinguish it from the 100-level retail release, and provide ordering
      information. This entry preserves the exact unchanged demo, not the retail
      game or a modified copy. No express redistribution prohibition appears in
      the included Read Me.
    notes: >-
      Original 493,822-byte StuffIt archive, SHA-256
      64889d7c946395629f76fa38083d2d756e75ead5df233497ea9b12bcc6894640.
      Contains the 68K application, four-level data, graphics, music, and the
      original Psygnosis Read Me.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/oh-no-more-lemmings/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3067
    permission: >-
      Fresh gameplay capture made for this catalogue entry from the unchanged
      promotional demo. Underlying game artwork remains its owner's property.
    notes: >-
      Exact 640-by-400 game-content crop at (80,104) of a deterministic
      800-by-600 Systemless framebuffer during live Level 1. The Mac menu bar
      and desktop were excluded without changing game pixels. PNG SHA-256
      34229beeedfc3676b16b5d23a411a1ccddd23ee5e9dbb6aceb87cf67a94b60c3,
      62,814 bytes.
references:
- https://classicmacdemos.com/oh-no-more-lemmings
- https://static.classicmacdemos.com/demos/oh-no-more-lemmings/README.txt
---

## Citizen Lemming

![Oh No! More Lemmings Level 1 gameplay](incoming/oh-no-more-lemmings/gameplay.png)

The first level asks you to save at least half of fifty Lemmings. Assign
limited skills to individual walkers, guide them across the pipe-filled
terrain, and keep an eye on the four-minute clock. This screenshot shows the
first lemming walking after the level begins.

This is Psygnosis's original four-level Macintosh demonstration, not the
100-level retail expansion. Its included Read Me calls it The Demo Disk and
explains the difference. Systemless reached the menu, level briefing, and
live gameplay using the unchanged 68K archive. Browser launch remains disabled
until a manual check is complete.
