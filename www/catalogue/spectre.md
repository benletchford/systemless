---
id: spectre
kind: game
title: Spectre
summary: >-
  Enter Peninsula Gameworks' original Macintosh tank arena in its promotional
  demo.
developer: Peninsula Gameworks
publisher: Velocity Development
year: 1991
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.53.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged Macintosh demo at 800-by-600 in 256
      colours. Passed the title screen and vehicle selection to reach the live Level 1
      tank view. Browser launch has not been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2488
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: e19a51c3d3f765a54fde79f848faa605c106c8c3309cb55ebd8e4e3fa334fdae
    size_bytes: 259401
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/spectre
    - https://www.wap.org/journal/showcase/washingtonapplepijournal1992v14no3mar92.pdf
    rights_holder: Spectre rights holders
    permission: >-
      This unchanged demonstration was distributed publicly to promote the original
      Macintosh game. The archive contains an application called Spectre Demo and a
      Description identifying it as the demonstration version. A 1992 Apple user-group
      listing independently describes a working Spectre demo. No retail game, serial, or
      editor is included; the archive has no express redistribution clause.
    notes: >-
      Original 259,401-byte StuffIt demo archive, SHA-256
      e19a51c3d3f765a54fde79f848faa605c106c8c3309cb55ebd8e4e3fa334fdae. MD5 5664ead985c70b064f3b83c12dfc5d47
      matches Macintosh Garden's SpectreDemo.sit listing. This is Spectre, not the later
      Spectre VR.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 206212fca43763ed6c3260e2a30a8f5ef67bc5e61a1308f53419b15b366884a4
    size_bytes: 7951
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2488
    permission: >-
      Fresh deterministic gameplay capture made for this catalogue entry. Underlying
      game artwork remains the property of its rights holders.
    notes: >-
      Captured the 640-by-480 game-content region at (80,60) after selecting a
      vehicle and entering Level 1. The crop excludes host UI and the Mac menu bar without
      changing game pixels. PNG SHA-256
      206212fca43763ed6c3260e2a30a8f5ef67bc5e61a1308f53419b15b366884a4, 7,951 bytes.
references:
- https://macintoshgarden.org/games/spectre
- https://www.wap.org/journal/showcase/washingtonapplepijournal1992v14no3mar92.pdf
---

## Into the tank arena

![Spectre demo Level 1 tank view](https://assets.systemless.org/catalogue/media/sha256/20/206212fca43763ed6c3260e2a30a8f5ef67bc5e61a1308f53419b15b366884a4.png)

Choose a tank and drive through a polygonal arena, collecting flags while
avoiding enemy tanks. The demo reaches the original game's Level 1 view, with
lives, damage, ammunition, score and radar around the 3D playfield.

## The original demo

This is Peninsula Gameworks' original Macintosh demonstration of Spectre,
not the retail game or the later Spectre VR demo. The archive contains only
the demo application and its short description.
