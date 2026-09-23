---
id: spaceward-ho
kind: game
title: Spaceward Ho!
summary: Explore, settle and contest a galaxy in Delta Tao's original Macintosh strategy demo.
developer: Delta Tao Software
publisher: Delta Tao Software
year: 1993
architectures: [68k]
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: '2026-09-23'
    tester: Catalogue maintainer
    systemless_version: 0.52.0-dev
    architecture: 68k
    environment: >-
      Deterministic headless play from the unchanged 3.0 demo archive, through
      galaxy creation and player setup to the live map; startup compared with
      BasiliskII System 8.1.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2470
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://gardenmirror.oldapplestuff.com/games/SpacewardHo_v3.0Demo.sit
    expected_sha256: aa58b78cb31e7be671ec84a4cc38bc3adc4d553a02adf20b6935e6bcee3c9a85
    expected_size: 401674
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/spaceward-ho
    - https://groups.google.com/g/comp.sys.mac.games/c/-B3pUZJLoGg
    - https://www.ho.deltatao.com/
    - https://apps.apple.com/us/app/spaceward-ho/id510527152
    license: Delta Tao Software 3.0 demo redistribution permission
    rights_holder: Delta Tao Software
    permission: >-
      The original demo's introductory dialog explicitly permits free
      distribution of this demo without charge. This applies to the unchanged
      demo package, not any complete commercial edition.
    notes: >-
      Unchanged 401,674-byte StuffIt 3.0 demo archive, SHA-256
      aa58b78cb31e7be671ec84a4cc38bc3adc4d553a02adf20b6935e6bcee3c9a85.
      A contemporary FAQ documents the public demo. Delta Tao still identifies
      Spaceward Ho! as its product, while a separately developed iOS adaptation
      credited to Delta Tao and Ariton is currently sold; neither that adaptation
      nor a complete classic edition is hosted here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/spaceward-ho/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2470
    permission: >-
      Original gameplay capture made for this catalogue at the maintainer's
      request. Underlying game artwork remains Delta Tao Software's property.
    notes: >-
      Deterministic capture from the exact unchanged 3.0 demo archive after
      joining a new galaxy. The 500-by-320 game-map surface was captured
      directly from the framebuffer at (300,74), excluding the Mac menu bar,
      desktop and window chrome without altering game pixels. PNG SHA-256
      775fc77bcf7db26e728893620aee2eb3bd569dd3a45c43d3ac83dfd4fbb7255c,
      30,856 bytes.
references:
- https://www.ho.deltatao.com/
- https://apps.apple.com/us/app/spaceward-ho/id510527152
- https://groups.google.com/g/comp.sys.mac.games/c/-B3pUZJLoGg
- https://macintoshgarden.org/games/spaceward-ho
---

## Settle the stars

![Spaceward Ho! galaxy gameplay](incoming/spaceward-ho/gameplay.png)

Spaceward Ho! begins by letting you shape the galaxy: choose its density and
layout, set the computer opponents' strength and decide how many will share the
map. Once inside, each named planet is a possible colony, resource source or
frontier. Your home world supplies the first ships and budget, while the map
quickly fills with decisions about exploration, technology and defence.

## The original 3.0 demo

This is Delta Tao's unchanged Macintosh demonstration, not the complete game.
Its own opening screen permits the demo to be given away without charge. The
catalogue preserves that exact package; the later commercial editions,
including the currently sold iOS adaptation, are not included.
