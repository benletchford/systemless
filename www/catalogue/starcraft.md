---
id: starcraft
kind: game
title: StarCraft Demo
summary: >-
  Command Terran forces in Blizzard's original Power Macintosh StarCraft
  demonstration.
developer: Blizzard Entertainment
publisher: Blizzard Entertainment
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: 0.59.0-dev
    architecture: ppc
    environment: >-
      Deterministic local PowerPC play of the unchanged 1.05 demo at 800 by 600, 256
      colours. Created a profile, entered the Terran Prequel mission, dismissed its
      tip, and selected an SCV on the live map. Browser review is pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2717
runtime:
  worker: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: d5bb967549d94ba9631763b11de38db11dcfcf17fbccf3722abaf60cd1796786
    size_bytes: 30188583
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/starcraft
    - https://static.classicmacdemos.com/demos/starcraft/README.txt
    - >-
      https://www.blizzard.com/en-sg/legal/c1ae32ac-7ff9-4ac3-a03b-fc04b8697010/blizzard-legal-faq
    rights_holder: Blizzard Entertainment, Inc.
    permission: >-
      Blizzard's legal FAQ permits noncommercial mirroring of its unaltered demos
      with every original file intact, subject to revocation. This is the unchanged
      historical promotional demo, not retail game media.
    notes: >-
      Original 30,188,583-byte StuffIt archive, SHA-256
      d5bb967549d94ba9631763b11de38db11dcfcf17fbccf3722abaf60cd1796786. Its bundled Read Me identifies version 1.05,
      dated 12 March 1999, and requires a Power Macintosh with System 7.6 or later.
      The game executable contains PowerPC PEF code and only a small 68K launch stub.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ea39bdae3357374fc093eb806fa6d943cc75ca97238bc59278e7a9cb9d7458a5
    size_bytes: 245307
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2717
    permission: >-
      Original gameplay capture made for this catalogue at the maintainer's request.
      Underlying game artwork remains Blizzard's property.
    notes: >-
      Deterministic capture from the exact unchanged demo archive on 25 September
      2026 after starting the Terran Prequel mission, dismissing its tip and selecting an
      SCV. Cropped from the 800-by-600 framebuffer to the 640-by-480 game surface
      without altering game pixels. PNG SHA-256
      ea39bdae3357374fc093eb806fa6d943cc75ca97238bc59278e7a9cb9d7458a5, 245,307 bytes.
references:
- https://classicmacdemos.com/starcraft
- https://static.classicmacdemos.com/demos/starcraft/README.txt
- https://github.com/benletchford/systemless/issues/2717
- https://github.com/benletchford/systemless/issues/2857
---

## Terran Prequel

![StarCraft Terran Prequel gameplay](https://assets.systemless.org/catalogue/media/sha256/ea/ea39bdae3357374fc093eb806fa6d943cc75ca97238bc59278e7a9cb9d7458a5.png)

Blizzard's original Macintosh demonstration includes a playable Terran
mission. Create a player profile, choose the mission, and use the command panel
to direct workers and troops across the map. The demo opens with a cinematic
introduction and mission briefing; Escape advances past the introduction.

This is the version 1.05 Power Macintosh demo, with its original game data and
Read Me preserved in the archive. Systemless reaches the live mission and
accepts unit selection in a deterministic local run. A local browser worker
preview boots and renders the title screen without a long main-thread startup
stall, but still runs below the guest-tick launch target.
The public browser route will be enabled after browser review.
