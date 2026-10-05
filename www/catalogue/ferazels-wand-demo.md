---
id: ferazels-wand-demo
kind: game
title: Ferazel's Wand Demo 1.0.3
summary: Explore the opening three levels of Ambrosia's hand-drawn PowerPC platform game.
developer: Ben Spees
publisher: Ambrosia Software
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: true
compatibility:
  status: boots
  verified:
  - date: "2026-10-05"
    tester: Catalogue maintainer
    systemless_version: 0.76.1 + deterministic play runner
    architecture: ppc
    environment: >-
      The original StuffIt demo installer opens its display prompt and New Game menu,
      then reaches the first playfield. Holding keypad 6 visibly moves Ferazel.
      Further gameplay remains unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4054
  - date: "2026-10-05"
    tester: Catalogue maintainer
    systemless_version: 0.78.0 + local browser preview
    architecture: ppc
    environment: >-
      A temporary same-origin copy of the exact archive loaded in the browser, passed
      the display prompt and New Game menu, and rendered the first stage. Repeated
      right-arrow input visibly moved Ferazel through the configured arrow-to-keypad
      mapping. No browser warnings or errors appeared. The promoted archive responds to
      a GET request from https://systemless.org with the expected size and browser access
      header. Later levels remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4054
controls:
  arrows_as_numpad: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 7c9b8ee0d911bfdf33d1dd21b08df45f9fd505e1a165272c6a4cfc1185110dbb
    size_bytes: 22675042
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://archive.org/details/tucows_205573_Ferazel_s_Wand
    - https://www.macintoshrepository.org/4577-ferazel-s-wand
    license: Ferazel's Wand Demo License
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      The bundled demo licence grants unlimited free use and permits nonprofit
      distribution without prior written notice when the complete software is included and
      left unmodified. Distribution for profit requires explicit written permission.
    notes: >-
      Original 22,675,042-byte StuffIt demo installer. Its SHA-1 is
      1da584be9db05cd2cd5eabfd6a8ab4eebee4c495, matching both the Internet Archive's Tucows file and the
      Macintosh Repository demo listing; its SHA-256 is
      7c9b8ee0d911bfdf33d1dd21b08df45f9fd505e1a165272c6a4cfc1185110dbb. The licence and three-level demo description
      were checked inside this exact installer.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0e594031ce426232d642eefbc01ee96c8914ac0a8e12583c60858626b0cf5d36
    size_bytes: 374287
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4054
    permission: >-
      Fresh gameplay capture from the complete, unchanged original demo installer.
      The underlying game artwork remains Ambrosia's property.
    notes: >-
      Cropped 608×472 game-content capture after a keypad-6 movement probe. It
      excludes emulator framing and the Classic Mac menu bar. PNG SHA-256
      0e594031ce426232d642eefbc01ee96c8914ac0a8e12583c60858626b0cf5d36; 374,287 bytes.
references:
- https://archive.org/details/tucows_205573_Ferazel_s_Wand
- https://www.macintoshrepository.org/4577-ferazel-s-wand
---

## Enter Teraknorn

![Ferazel in the opening demo level](https://assets.systemless.org/catalogue/media/sha256/0e/0e594031ce426232d642eefbc01ee96c8914ac0a8e12583c60858626b0cf5d36.png)

Ferazel walks, climbs, digs, and casts spells through the first three levels of
Ambrosia's original demo. The full commercial game is not included.

The bundled manual assigns keypad 4 and 6 to walking, Option to jumping, and
Command to the selected spell or item. This entry sends arrow keys as their
numeric keypad counterparts.
