---
id: spectre-vr
kind: game
title: Spectre VR
summary: >-
  Enter Velocity's first-person tank arena in the original Spectre VR CD Demo.
developer: Velocity Development
publisher: Velocity Development
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.51.0"
    architecture: 68k
    environment: >-
      Deterministic run from the unchanged Spectre VR Demo StuffIt archive,
      through the launcher and vehicle selection into a live Level 1 scene
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2445
runtime:
  application_partition_size: 8388608
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/spectre-vr/
    download_page: https://classicmacdemos.com/spectre-vr
    expected_sha256: a915af0974f0e99dbc01e462e3e5471c42cf817cfc6f9f13bd811b3f9e958dbb
    expected_size: 23339583
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/spectre-vr
    - https://classicmacdemos.com/download/spectre-vr/
    - https://static.classicmacdemos.com/demos/spectre-vr/README.txt
    license: Velocity Development Spectre VR promotional CD demo distribution
    rights_holder: Spectre VR rights holders
    permission: >-
      Velocity Development released this purpose-built Spectre VR CD demo. Its
      included read-me identifies it as a demo and distinguishes it from the
      retail game. Classic Macintosh Game Demos documents period demo-disc
      distribution and continues to offer the demo archive. Only the original
      promotional demo is staged, not the retail game.
    notes: >-
      Unchanged 23,339,583-byte StuffIt archive with SHA-256
      a915af0974f0e99dbc01e462e3e5471c42cf817cfc6f9f13bd811b3f9e958dbb.
      It contains the 68K Spectre VR CD Demo application and its bundled data.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/spectre-vr/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2445
    permission: >-
      Original in-game screenshot captured for this catalogue at the
      maintainer's request. Underlying Spectre VR artwork remains the property
      of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.51.0 capture from the staged demo on
      2026-09-23, after selecting a vehicle and entering Level 1. The 800x600
      guest framebuffer was captured as the 512x342 game viewport. PNG SHA-256
      bfc966663c63663a2073b5f38a737edaf243eeadf1250f390031a95712e694fd;
      46,047 bytes. The custom launcher control artwork is not yet rendered
      correctly in Systemless; see the compatibility evidence.
references:
- https://classicmacdemos.com/spectre-vr
- https://static.classicmacdemos.com/demos/spectre-vr/README.txt
---

## Into the arena

![Spectre VR CD Demo Level 1 tank view](incoming/spectre-vr/gameplay.png)

Spectre VR puts the player inside a tank for a first-person arena fight. The
demo's launcher leads through vehicle selection into Level 1, with a live
score, timer, damage meter and weapon display around the 3D view.

This is Velocity Development's original CD demo, not the commercial release.
Its included read-me describes the demo and what differs in the retail game.
The original StuffIt archive is preserved byte-for-byte. Systemless reaches the
live arena, but its custom launcher buttons currently lack their icon artwork;
the compatibility status therefore remains at “boots” pending a fuller
interactive check and a rendering fix.
