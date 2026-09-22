---
id: simcity-2000
kind: game
title: SimCity 2000
summary: Build, budget and reshape Demo City in Maxis's interactive city-building demonstration.
developer: Maxis
publisher: Maxis
year: 1993
architectures: [68k]
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: 2026-09-22
    tester: Catalogue maintainer
    systemless_version: 0.48.0
    architecture: 68k
    environment: >-
      Deterministic headless gameplay run from the unchanged original StuffIt
      demo archive, cross-checked through the same script under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/SimCity%202000%20Demo.sit
    expected_sha256: 499e54b44655bbcda54967f75de83cca86453b3d9533c7d0e5d5d7f26ee4339b
    expected_size: 1060825
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/simcity-2000
    - https://static.classicmacdemos.com/demos/simcity-2000/README.txt
    - https://www.ea.com/games/simcity/simcity-2000
    - https://www.gog.com/en/game/simcity_2000_special_edition
    license: Maxis SimCity 2000 Interactive Demo promotional distribution
    rights_holder: Electronic Arts Inc.
    permission: >-
      Maxis packaged this as a self-contained, time-limited Interactive Demo
      whose included ReadMe invites players to build, explore and restart it.
      The unchanged package was distributed on at least fourteen contemporary
      magazine and promotional discs. That documented no-charge distribution
      history supports continued redistribution of the exact demo package; it
      does not extend to the retail game, modified copies or paid bundles.
    notes: >-
      The unchanged 1,060,825-byte StuffIt archive has SHA-256
      499e54b44655bbcda54967f75de83cca86453b3d9533c7d0e5d5d7f26ee4339b.
      It contains Demo City, a 245-byte Maxis ReadMe and the 68K
      SimCity2000 Interactive Demo application. Electronic Arts currently
      maintains a SimCity 2000 product page, and the Special Edition remains
      commercially available through GOG. The catalogue hosts only this
      time-limited promotional demo, unchanged.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/simcity-2000/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2222
    permission: >-
      Original gameplay screenshot captured for this catalogue at the
      maintainer's request. Underlying game artwork remains the property of
      Electronic Arts Inc.
    notes: >-
      Fresh deterministic Systemless 0.48.0 capture made from the exact
      unchanged demo archive on 2026-09-22 after entering Demo City and using
      a construction tool. The 800x600 framebuffer was cropped to the 778x547
      game content surface, excluding the Mac menu bar, window title and
      scroll-bar chrome; no game pixels were altered. PNG SHA-256
      f60a859897501f6719a86a14b2b0006312fa14c52f72d44c89bf2ed47c2b5025,
      278,288 bytes.
references:
- https://classicmacdemos.com/simcity-2000
- https://static.classicmacdemos.com/demos/simcity-2000/README.txt
- https://www.ea.com/games/simcity/simcity-2000
- https://www.gog.com/en/game/simcity_2000_special_edition
---

## A living city in miniature

![SimCity 2000 gameplay](incoming/simcity-2000/gameplay.png)

The interactive demo opens Demo City as a working metropolis rather than a
scripted tour. Roads cross rail lines, power plants feed dense neighbourhoods,
traffic moves through the centre and the simulation continues to collect taxes
and update demand while the player changes the map.

This is a substantial demonstration of the original Macintosh game. Its tools
support zoning, roads, rails, power lines, parks and civic buildings. The map
can be rotated and zoomed, management windows expose population, industry and
neighbour data, and the disasters menu can interrupt an otherwise careful plan.

## Preserved without repacking

The source package is Maxis's original StuffIt archive. Systemless opens that
archive directly and discovers the application and Demo City file without a
converted disk image, extracted application or generated web package. A
deterministic run reaches the live city, dismisses the welcome and budget
dialogs, selects a construction tool and changes a visible map tile.

Electronic Arts is the present rights holder and still offers SimCity 2000
Special Edition commercially. This catalogue uses only Maxis's time-limited
promotional demo, preserved byte-for-byte; it does not substitute or expose the
retail game. The launcher remains disabled until its promoted archive and page
have passed the production and browser-review gates.
