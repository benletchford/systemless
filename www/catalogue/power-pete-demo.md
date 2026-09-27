---
id: power-pete-demo
kind: game
title: Power Pete Demo
summary: >-
  Rescue the toy-store bunnies in Pangea's original first-level Macintosh
  demonstration.
developer: Pangea Software
publisher: MacPlay / Interplay Productions
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 8869eb2f1ef843424bd15e7639981a94ae61728c
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged demo. The title screen opened
      and Play entered the first Jurassic level with Pete, enemies, and the full status
      panel visible. Enemies moved and the lives counter changed during the run.
      Deliberate character movement, native-Mac comparison, and browser launch have not yet
      been verified. The promoted archive and screenshot were fetched back and
      matched their recorded SHA-256 hashes.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3087
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 1176f1dbfb504f7c4c951206c4d09bb85368da5b7bfdd2494157efc3f28d10ee
    size_bytes: 1907851
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/power-pete
    - https://download.classicmacdemos.com/PowerPeteDemo.sit
    - https://static.classicmacdemos.com/demos/power-pete/README.txt
    rights_holder: Pangea Software and MacPlay / Interplay Productions
    permission: >-
      The bundled Read Me identifies this as Demo version 1.0 and says it contains
      only the first level of the commercial game. It includes contemporary ordering
      information and no express redistribution prohibition. This entry preserves the exact
      unchanged promotional archive, not the retail game.
    notes: >-
      Original 1,907,851-byte StuffIt archive, SHA-256
      1176f1dbfb504f7c4c951206c4d09bb85368da5b7bfdd2494157efc3f28d10ee. Includes the 68K demo application, graphics,
      audio, a Jurassic level map, and Pangea's Read Me. The Read Me distinguishes
      expanded Power Macintosh display mode from the standard Macintosh mode.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 1b2a08eeff81a994a61e7c84b8eedbd0f49462121975464e6ddca3df2d990615
    size_bytes: 414339
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3087
    permission: >-
      Fresh live-level capture made for this catalogue entry from the unchanged
      promotional demo. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 640-by-480 game-content crop at (80,60) of a deterministic 800-by-600
      Systemless framebuffer. Surrounding desktop was excluded without changing game
      pixels. PNG SHA-256 1b2a08eeff81a994a61e7c84b8eedbd0f49462121975464e6ddca3df2d990615,
      414,339 bytes.
references:
- https://classicmacdemos.com/power-pete
- https://static.classicmacdemos.com/demos/power-pete/README.txt
---

## Jurassic toy-store trouble

![Power Pete demo first level with Pete and enemies](https://assets.systemless.org/catalogue/media/sha256/1b/1b2a08eeff81a994a61e7c84b8eedbd0f49462121975464e6ddca3df2d990615.png)

Pete starts among toy-store enemies with a suction-cup gun. The first-level
view shows the terrain, moving foes, and a status panel tracking health, keys,
score, ammunition, lives, and bunnies still to rescue. The bundled Read Me
explains the mouse and keyboard controls and confirms this demo contains only
the first level.

This is Pangea's original Macintosh promotional demo, not the retail game.
Systemless reaches the live level from its unchanged 68K-capable archive.
Browser launch remains disabled until a manual check is complete.
