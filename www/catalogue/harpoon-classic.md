---
id: harpoon-classic
kind: game
title: Harpoon Classic
summary: Command a North Atlantic naval engagement in the original Macintosh demo.
developer: Alliance Interactive Software
publisher: Alliance Interactive Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: 0.52.0-dev
    architecture: 68k
    environment: >-
      Deterministic headless play of the unchanged Classic CD demo archive at
      800-by-600 in 256 colours. Selected the GIUK battleset, accepted game options, selected
      Dawn Patrol, and reached the live tactical map.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2477
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 546fe5eeb21f006596f2188caf861d020b18836d7e75f61c620aa7148c35fc70
    size_bytes: 952220
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/harpoon-classic
    - https://www.harpoonpages.com/harpoon1.htm
    - https://command.matrixgames.com/?page_id=530
    rights_holder: Applied Computing Services, Inc. and Alliance Interactive Software, Inc.
    permission: >-
      This unchanged, limited Macintosh demonstration was distributed publicly to
      promote Harpoon Classic. HarpoonPages still offers a contemporary full-working
      Macintosh demo and states that its play is limited to six scenarios; Macintosh Garden
      separately identifies this CD-demo archive as a demo. The archive itself contains
      no express redistribution clause. Hosting is limited to this original demo, not
      any retail edition.
    notes: >-
      Original 952,220-byte StuffIt archive, SHA-256
      546fe5eeb21f006596f2188caf861d020b18836d7e75f61c620aa7148c35fc70. MD5 56ed10636fbebbb78652be707dab4cfc matches
      Macintosh Garden's HarpoonClassicCDDemo.sit listing. Historical code and publishing
      rights involved ACSI and Alliance; no complete commercial game is included.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c070986090092d7e257a4b1f2715336f5ba29d3a349c1e53d387581d10d221b1
    size_bytes: 48102
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2477
    permission: >-
      Fresh deterministic gameplay capture made for this catalogue entry. Underlying
      game artwork remains the property of its rights holders.
    notes: >-
      Captured the 800-by-554 game-only framebuffer region at (0,20) after starting
      the Dawn Patrol scenario. The crop excludes the Mac menu bar and outer desktop
      without altering game pixels. PNG SHA-256
      c070986090092d7e257a4b1f2715336f5ba29d3a349c1e53d387581d10d221b1, 48,102 bytes.
references:
- https://macintoshgarden.org/games/harpoon-classic
- https://www.harpoonpages.com/harpoon1.htm
- https://command.matrixgames.com/?page_id=530
---

## Patrol the GIUK gap

![Harpoon Classic demo tactical map](https://assets.systemless.org/catalogue/media/sha256/c0/c070986090092d7e257a4b1f2715336f5ba29d3a349c1e53d387581d10d221b1.png)

*Harpoon Classic* begins with a battleset and scenario selection. Dawn Patrol
places a small NATO surface group in the waters between Greenland, Iceland,
and the United Kingdom. The tactical view shows the coastline, contact and
unit displays, formation controls, speed, and course as the clock advances.

## The Macintosh demo

This is Alliance Interactive's limited demonstration, not the complete
commercial game. A [contemporary Harpoon resource
page](https://www.harpoonpages.com/harpoon1.htm) describes its publicly
distributed Macintosh demo as playable but scenario-limited. The catalogue
preserves this unchanged CD-demo archive and its supporting resource files;
retail battlesets and later editions are not included.
