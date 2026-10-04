---
id: meteor-storm
kind: game
title: Meteor Storm 1.4
summary: >-
  Defend your ship against waves of enemies in Z Sculpt Entertainment's
  two-player space shooter.
developer: Zachary Black
publisher: Z Sculpt Entertainment
year: 2000
architectures:
- ppc
default_architecture: ppc
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-10-05"
    tester: Catalogue maintainer
    systemless_version: 0.76.1 deterministic play runner
    architecture: ppc
    environment: >-
      The original Info-Mac 1.4 package starts its native PowerPC code, accepts the
      640x480 display change, reaches the main menu, and starts Wave 1. Command enters
      the first player into gameplay with a visible ship and HUD. A local browser
      preview of the same archive reaches the shareware notice and main menu, but the canvas
      remains black after Start Game. Sustained play and period Macintosh comparison
      remain open.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4021
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 60e5a8c365fd18f99d72800a6b0858a3947b6d6b8d8ef54f30a2bfeeabc9716a
    size_bytes: 3872023
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/_Arcade/meteor-storm-14.hqx
    - https://github.com/benletchford/systemless/issues/4021
    license: Meteor Storm 1.4 bundled Read Me distribution terms
    rights_holder: Z Sculpt Entertainment
    permission: >-
      The bundled Read Me permits distribution of Meteor Storm as shareware when the
      software package is complete and unmodified. This original archive includes the
      Read Me and does not include an MS UserKey.
    notes: >-
      Original 3,872,023-byte BinHex archive, SHA-256
      60e5a8c365fd18f99d72800a6b0858a3947b6d6b8d8ef54f30a2bfeeabc9716a.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8c55d7fb2b57d397ce9280f47f377d81ee21c2c6e0eedcbb78e33ab613253912
    size_bytes: 10872
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4021
    permission: >-
      Original gameplay capture from the unregistered shareware package. Underlying
      game artwork remains its owner's property.
    notes: >-
      640x480 content-only Wave 1 capture from this exact Info-Mac archive running
      its native PowerPC code. SHA-256
      8c55d7fb2b57d397ce9280f47f377d81ee21c2c6e0eedcbb78e33ab613253912.
references:
- >-
  https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/_Arcade/meteor-storm-14.hqx
- https://github.com/benletchford/systemless/issues/4021
---

## Defend against the first wave

![Meteor Storm Wave 1 gameplay](https://assets.systemless.org/catalogue/media/sha256/8c/8c55d7fb2b57d397ce9280f47f377d81ee21c2c6e0eedcbb78e33ab613253912.png)

Choose **Start Game** from the main menu, then press **Command** to join as
player one. The main menu includes a help screen with the movement, sound,
and two-player controls.

This is the original unregistered shareware release. The bundled Read Me
explains how to register and permits redistribution of the complete,
unmodified package.
