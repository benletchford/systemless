---
id: empire-deluxe-demo
kind: game
title: Empire Deluxe Demo
summary: >-
  Explore an unknown world, capture cities and build armies, fleets and air
  forces in the classic turn-based conquest game.
developer: White Wolf Productions
publisher: New World Computing
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged demo archive ran natively on the 68K slice. File > Play > Basic
      Rules > Person versus Computer, with Return for the name and setup dialogs,
      started a game on the partly explored map. The numeric keypad then moved the map
      cursor. A matched idle control was identical until the input and then differed by
      33,485 pixels. An exact replay matched every checkpoint, with zero exhausted frames.
      Combat, production, audio remain unverified. Three browser samples on the intro
      scene passed at about 60 host fps and 60 guest ticks/s; in-game pacing was not
      separately measured.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5520
runtime:
  show_menu_bar: true
controls:
  arrows_as_numpad: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 647be14dd5fc0abe6e542009f9b8c7b913d7e0bab89270d366fdb738377d761a
    size_bytes: 1999580
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/empire-deluxe
    - https://download.classicmacdemos.com/Empire%20Deluxe%20Demo.sit
    - https://github.com/benletchford/systemless/issues/5520
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: New World Computing
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: 68K demo of the turn-based world-conquest strategy game.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 5d68bfd9ce01d5d504089c389cb727747673e065ecf5ef221e4b708ef801d755
    size_bytes: 13910
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5520
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: >-
      Native 68K opening turn after Person versus Computer setup, cropped to the game
      window.
references:
- https://classicmacdemos.com/empire-deluxe
- https://github.com/benletchford/systemless/issues/5520
---

## Conquer the world

![Empire Deluxe Demo](https://assets.systemless.org/catalogue/media/sha256/5d/5d68bfd9ce01d5d504089c389cb727747673e065ecf5ef221e4b708ef801d755.png)

Start with a single city on a map shrouded in darkness. Produce units, explore, take neutral cities and race your rivals to control the world.

Choose File > Play > Basic Rules to start. Move units with the arrow keys or the numeric keypad.
