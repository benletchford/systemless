---
id: glider-pro
kind: game
title: Glider PRO
summary: >-
  Fly a paper airplane through the Demo House in Casady & Greene's original
  Macintosh demonstration of Glider PRO.
developer: John Calhoun
publisher: Casady & Greene, Inc.
year: 1994
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
      Deterministic run from the unchanged Glider PRO Demo StuffIt archive through
      its title screen and animated Demo House, including multiple rooms
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2442
runtime:
  application_partition_size: 4194304
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: c54b543d56ca0a1e05ab201e379d2411cf09a47ea086caadcd24b040a59dd7ae
    size_bytes: 1268279
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/glider-pro
    - https://classicmacdemos.com/download/glider-pro/
    - https://static.classicmacdemos.com/demos/glider-pro/README.txt
    license: Casady & Greene Glider PRO promotional demo distribution
    rights_holder: Glider PRO rights holders
    permission: >-
      Casady & Greene released this purpose-built Glider PRO demo. Its included
      read-me calls it a demo, explains the Demo House and controls, distinguishes it from
      the retail release, and provides ordering information. Classic Macintosh Game
      Demos documents contemporary distribution on 19 demo discs and continues to offer
      this demo archive. Only the original demo is staged, not the retail game or later
      Carbon beta.
    notes: >-
      Unchanged 1,268,279-byte StuffIt archive with SHA-256
      c54b543d56ca0a1e05ab201e379d2411cf09a47ea086caadcd24b040a59dd7ae. It contains the 68K Glider PRO Demo
      application, Demo House and Getting Started file.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 26055cd5897fce1fe8bcd599cc11432eaa29709507acb79ac5ae601303b18e09
    size_bytes: 138701
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2442
    permission: >-
      Original in-game screenshot captured for this catalogue at the maintainer's
      request. Underlying Glider PRO artwork remains the property of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.51.0 capture from the staged demo archive on
      2026-09-23 during the animated Demo House. The 800x600 guest framebuffer was
      captured as the 800x342 game-only viewport, excluding the surrounding desktop. PNG
      SHA-256 26055cd5897fce1fe8bcd599cc11432eaa29709507acb79ac5ae601303b18e09; 138,701
      bytes.
references:
- https://classicmacdemos.com/glider-pro
- https://static.classicmacdemos.com/demos/glider-pro/README.txt
---

## Through the Demo House

![A paper airplane in the Glider PRO Demo House](https://assets.systemless.org/catalogue/media/sha256/26/26055cd5897fce1fe8bcd599cc11432eaa29709507acb79ac5ae601303b18e09.png)

Glider PRO sends a paper airplane drifting through a house of vents, candles,
windows and other hazards. Air currents keep the glider aloft; the player steers
with the arrow keys, searches for lift and threads a path between rooms.

This is Casady & Greene's original Macintosh demo, with its purpose-built Demo
House, not the commercial release. The included read-me invites players to watch
the house's automatic tour before trying it themselves and explains which rooms,
objects and house-editing features are reserved for the full version. The
catalogue preserves the promotional StuffIt archive byte-for-byte and launches
its original 68K application.
