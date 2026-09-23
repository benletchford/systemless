---
id: glypha
kind: game
title: Glypha
summary: Joust through an Egyptian-themed arena in John Calhoun's original monochrome Macintosh shareware game.
developer: John Calhoun
publisher: Soft Dorothy Software
year: 1990
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.54.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged Glypha 3.0 Macintosh archive
      against the clean public runtime. File > Begin reached the animated
      arena at both the native 512-by-342 resolution and the default 800-by-600
      profile. Browser interaction has not yet been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2507
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.grenier-du-mac.net/telechargement/Glypha/Glypha.sit
    download_page: https://www.grenier-du-mac.net/fiches/Jeux/glypha.htm
    expected_sha256: 9d4dc1ebb9fdded3ecb5bb57ea58c05033f2def7f36d5e085e902532015e4bf4
    expected_size: 151100
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.grenier-du-mac.net/fiches/Jeux/glypha.htm
    - https://github.com/EngineersNeedArt/SoftDorothy-SharewareProjects
    rights_holder: John Calhoun
    permission: >-
      This is the unchanged original monochrome Glypha 3.0 shareware
      application, not the later commercial Glypha: Vintage remake. Author
      John Calhoun describes Glypha as a shareware game released freely via
      FTP sites and has publicly released its source and original development
      disk. Grenier du Mac preserves this original build as freeware. The
      archive contains no separate express redistribution license.
    notes: >-
      Original 151,100-byte StuffIt archive containing Glypha 3.0, SHA-256
      9d4dc1ebb9fdded3ecb5bb57ea58c05033f2def7f36d5e085e902532015e4bf4.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/glypha/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2507
    permission: >-
      Fresh deterministic game-board capture made for this catalogue entry.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Captured directly from live File > Begin gameplay on a 512-by-342 screen.
      The 512-by-322 game-content rectangle at (0,20) excludes the Classic Mac
      menu bar without altering game pixels. PNG SHA-256
      c0bfab038a3ceff939f77b714438d4a11588cd348a6a2908e011683dbd64575c,
      58,952 bytes.
references:
- https://www.grenier-du-mac.net/fiches/Jeux/glypha.htm
- https://github.com/EngineersNeedArt/SoftDorothy-SharewareProjects
---

## Egyptian jousting on a classic Mac

![Glypha 3.0 Macintosh arena](incoming/glypha/gameplay.png)

Ride a flying bird through Glypha's single-screen arena, trying to take the
higher position over enemies and collect their eggs before they hatch. John
Calhoun's original monochrome game was built for the early Macintosh.

This entry uses the original freely distributed shareware build, not its
modern commercial remake. Systemless reaches the arena; browser launch
awaits manual approval.
