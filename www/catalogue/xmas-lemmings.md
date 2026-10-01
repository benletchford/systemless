---
id: xmas-lemmings
kind: game
title: Xmas Lemmings
summary: Guide Santa-hatted lemmings through Psygnosis's four-level 1992 Christmas demo.
developer: DMA Design
publisher: Psygnosis
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged StuffIt demo opened its introduction and menu, then Level 1
      Jingle Lemming. Lemmings walked and the level timer advanced. Matched replays showed
      the release-rate control at 50 without input and 51 after one click.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3711
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once, passed both introduction screens,
      and opened live Level 1. A click raised the visible release rate from 50 to 51. A
      five-second active-level sample measured 60.0 host frames/s and 60.0 guest
      ticks/s, with an 18.2 ms maximum frame and 193 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3711
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 01a6bb8f4a43715b6fefba3f3b99546497e00b6ffd9ff1805f311fa8de216c70
    size_bytes: 611587
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/xmas-lemmings
    - https://www.playstation.com/en-gb/legal/copyright-and-trademark-notice/
    rights_holder: Sony Interactive Entertainment Europe Limited
    permission: >-
      Psygnosis presented this original four-level 1992 Christmas demo as a gift on
      its opening screen. The archive contains the 68K application and game resources,
      with no bundled redistribution restriction. This entry uses the unchanged demo
      rather than a retail release.
    notes: >-
      Original 611,587-byte StuffIt archive, SHA-256
      01a6bb8f4a43715b6fefba3f3b99546497e00b6ffd9ff1805f311fa8de216c70.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: aa7fbd83f92160c72eca3480401e5e764946344dca57b04063ec8b66c95d0b74
    size_bytes: 54414
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3711
    permission: >-
      Fresh gameplay capture made from the unchanged demonstration for this catalogue
      entry. Underlying game artwork remains its owner's property.
    notes: >-
      Exact 640-by-400 game-content crop at (80,104) of an 800-by-600 Systemless
      framebuffer during live Level 1 after increasing release rate.
references:
- https://classicmacdemos.com/xmas-lemmings
---

## Jingle Lemming

![Santa-hatted lemmings walking through Jingle Lemming](https://assets.systemless.org/catalogue/media/sha256/aa/aa7fbd83f92160c72eca3480401e5e764946344dca57b04063ec8b66c95d0b74.png)

The first level releases fifty lemmings into a snowy landscape. Change the
release rate or assign skills to guide enough of them to the exit before time
runs out. Click through the two opening screens, choose **Let's Go**, and
dismiss the level briefing to play.

This is Psygnosis's original four-level 1992 Christmas demo for Macintosh.
