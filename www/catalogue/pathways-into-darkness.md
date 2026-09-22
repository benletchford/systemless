---
id: pathways-into-darkness
kind: game
title: Pathways into Darkness
summary: >-
  Descend beneath the Yucatán in Bungie's original Macintosh demonstration of its
  landmark first-person adventure.
developer: Bungie Software Products Corporation
publisher: Bungie Software Products Corporation
year: 1993
architectures:
- 68k
default_architecture: 68k
category: FPS
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic run from the unchanged Pathways into Darkness demo v2.0 archive
      through new-game creation and first-run instructions into movable Ground Floor
      gameplay, cross-checked with the same script under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2436
runtime:
  application_partition_size: 16777216
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 0b3f8529367e881fbb7cc402d37f2ce9abe56eb4e2089fd31169cd1f87e61bc2
    size_bytes: 937427
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/pathways-into-darkness
    - https://download.classicmacdemos.com/Pathways%20Demo.sit
    - https://pid.bungie.org/demoV2.0ReadMe.html
    license: Bungie Pathways into Darkness promotional demo distribution
    rights_holder: Bungie, Inc. and its successors
    permission: >-
      Bungie deliberately distributed this self-contained package as Pathways into
      Darkness Demo v2.0. Its included March 1994 read-me identifies it as a demo,
      documents the packaged application and data files, advertises where to buy the retail
      game, and provides Bungie's contemporary contact details. Classic Macintosh Game
      Demos records the package on 33 period demo discs and continues to distribute it
      specifically as the playable demo. This entry preserves only those demo files and
      does not include the retail game.
    notes: >-
      Unchanged 937,427-byte StuffIt archive with SHA-256
      0b3f8529367e881fbb7cc402d37f2ce9abe56eb4e2089fd31169cd1f87e61bc2. The package retains the original demo
      application, maps, shapes, sounds, saved games, icon and read-me.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7538964caa7763bdebe9a2b043714bd84972c2ccbb77646a55552d51eafb7214
    size_bytes: 76582
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2436
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request. Underlying Pathways into Darkness artwork remains the property of its
      rights holders.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the staged demo archive
      on 2026-09-23 after creating a new game and moving through the Ground Floor. The
      800x600 guest framebuffer was cropped to the 631x452 game workspace, excluding
      the Classic Mac menu bar and surrounding desktop. PNG SHA-256
      7538964caa7763bdebe9a2b043714bd84972c2ccbb77646a55552d51eafb7214, 76,582 bytes.
references:
- https://classicmacdemos.com/pathways-into-darkness
- https://pid.bungie.org/demoV2.0ReadMe.html
- https://marathon.bungie.org/story/Pathways_Demo_Instructions_4Jul93.html
---

## Into the pyramid

![Exploring the Ground Floor in Pathways into Darkness](https://assets.systemless.org/catalogue/media/sha256/75/7538964caa7763bdebe9a2b043714bd84972c2ccbb77646a55552d51eafb7214.png)

A Special Forces mission has gone badly wrong above an ancient pyramid in the
Yucatán. Alone and poorly equipped, the player must descend through the tunnels,
recover the missing nuclear device and stop a sleeping alien presence from
waking. Real-time texture-mapped exploration shares the screen with inventory,
health, messages and weapon skills, joining first-person action to the careful
resource management of an adventure game.

The demo supplies three purpose-built levels and several saved positions. Its
opening Ground Floor introduces movement, searchable remains and scattered
equipment before the route leads deeper underground.

## Bungie's Power Macintosh demo

This is Bungie's original Demo v2.0 package, not the commercial game. The
included March 1994 read-me calls it the Pathways into Darkness demo, lists its
four required application and data files, explains the controls and directs
players to contemporary retailers for the complete release. It was distributed
widely: Classic Macintosh Game Demos identifies copies on 33 period demo discs.

The catalogue stages the preserved StuffIt archive byte-for-byte. Systemless
extracts the package and runs the original 68K application, creates a new game,
dismisses the first-run guidance and enters the live Ground Floor. A matching
BasiliskII run independently confirms the archive, interactions and rendered
scene.
