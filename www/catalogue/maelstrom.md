---
id: maelstrom
kind: game
title: Maelstrom
summary: >-
  Steer a lone ship through drifting rocks and increasingly crowded waves of
  arcade mayhem.
developer: Andrew Welch
publisher: Ambrosia Software
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: 2026-09-15
    tester: Catalogue maintainer
    systemless_version: 0.41.2
    architecture: 68k
    environment: Deterministic headless framebuffer run from the original installer
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/131
controls:
  mobile:
    buttons:
    - key: Space
      label: Fire
    - key: Tab
      label: Shld
    - key: P
      label: Go
    enabled: true
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 194be3d54a5969336c99260829e808f5a2af1a19ce720bacc80bbc0bad507fa0
    size_bytes: 1156480
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macos.retro-os.live/jdownloads/LatestFiles.html
    - https://macos.retro-os.live/jdownloads/MacOS%20Files/Color%20Apps/Maelstrom_1.4.3_Installer.bin
    - https://www.macintoshrepository.org/4478-maelstrom
    - https://sources.debian.org/src/maelstrom/1.4.3-L2.0.6-13/Doc/Ambrosia.FAQ
    license: Ambrosia Software shareware
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      Ambrosia's shareware FAQ encourages users to copy its products and give them to
      friends, subject to paying after the 30-day evaluation period.
    notes: >-
      Untouched Maelstrom 1.4.3 MacBinary installer independently retrieved on
      2026-09-15. The 1,156,480-byte file has SHA-256
      194be3d54a5969336c99260829e808f5a2af1a19ce720bacc80bbc0bad507fa0. Its
      SHA-1, 1413d439dd4a25ecaa8f968f67c920aeab3d7e98, matches the separately
      catalogued copy at Macintosh Repository.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ef2b0b08aee0d9911809da897400cacb77002810877349dbdff365066284796e
    size_bytes: 17721
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/131
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh unedited framebuffer capture made from the unchanged installer on
      2026-09-15. It shows live play and excludes browser, website, host desktop, emulator
      controls and Classic Mac desktop furniture.
references:
- https://sources.debian.org/src/maelstrom/1.4.3-L2.0.6-13/Doc/Ambrosia.FAQ
- https://github.com/libsdl-org/Maelstrom
---

## Source and version

This is Maelstrom 1.4.3 in its original MacBinary installer. It remains sealed as Ambrosia distributed it; Systemless opens the installer and finds the game at launch time.

## In the asteroid field

![Maelstrom gameplay](https://assets.systemless.org/catalogue/media/sha256/ef/ef2b0b08aee0d9911809da897400cacb77002810877349dbdff365066284796e.png)

Maelstrom begins with the lovely economy of an arcade cabinet: one small ship, a screenful of rock, and nowhere to hide. The first wave leaves room to learn the ship's momentum. Later waves fill that quiet black field with splintered asteroids, enemy craft and bonus capsules until survival becomes a rhythm of thrust, turn, fire and last-second shielding.

The details give it its own character. Rocks tumble rather than merely slide, the ship carries visible damage and every cleared wave grants only a short breath before the next arrives. High scores sit on the opening screen as both invitation and accusation.

## Shareware

Ambrosia released the complete game as shareware with a 30-day evaluation period. Its contemporary FAQ actively encouraged people to copy the software for friends, while asking players who kept it to register.
