---
id: prince-of-persia
kind: game
title: Prince of Persia
summary: Run, leap and climb through the first dungeon in Brøderbund's playable Macintosh demonstration.
developer: Presage Software
publisher: Brøderbund Software
year: 1992
architectures: [68k]
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: 2026-09-22
    tester: Catalogue maintainer
    systemless_version: 0.48.0
    architecture: 68k
    environment: >-
      Deterministic gameplay run from the unchanged original StuffIt demo
      archive, cross-checked through the same script under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Prince%20of%20Persia.sit
    expected_sha256: 1914f6bd17b4b9ff0ae68cf5408ddf88ebfc2ae6c85a8b5499fde5d52ae31917
    expected_size: 1153091
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/prince-of-persia
    - https://static.classicmacdemos.com/demos/prince-of-persia/README.txt
    - https://www.jordanmechner.com/en/games-movies/prince-of-persia/
    - https://www.ubisoft.com/en-us/game/prince-of-persia
    license: Brøderbund Prince of Persia promotional demo distribution
    rights_holder: Ubisoft Entertainment
    permission: >-
      Brøderbund packaged this as a self-contained Prince of Persia Demo with
      distinct black-and-white, colour and Macintosh LC game data. The bundle
      includes a Brøderbund Read Me and a Where to Buy document giving product
      ordering details. Its deliberate promotional construction and documented
      no-charge distribution support continued redistribution of this exact
      demo package; that basis does not extend to the retail game, cracks or
      modified copies.
    notes: >-
      The unchanged 1,153,091-byte StuffIt archive has SHA-256
      1914f6bd17b4b9ff0ae68cf5408ddf88ebfc2ae6c85a8b5499fde5d52ae31917.
      It contains the 68K Prince of Persia Demo application, three original
      display-specific data files, a 2,726-byte Read Me and a 3,246-byte Where
      to Buy document. Jordan Mechner identifies himself as the game's original
      creator and Brøderbund as its 1989 publisher; Ubisoft maintains the
      current franchise. The catalogue hosts only this promotional demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/prince-of-persia/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2222
    permission: >-
      Original gameplay screenshot captured for this catalogue at the
      maintainer's request. Underlying game artwork remains the property of
      Ubisoft Entertainment.
    notes: >-
      Fresh deterministic Systemless 0.48.0 capture made from the exact
      unchanged demo archive on 2026-09-22 after starting Level 1 and moving
      the Prince through the first chamber. The 800x600 framebuffer was cropped
      to the 640x400 game surface, removing only uniform black margins; no game
      pixels were altered. PNG SHA-256
      fe82086f572a8043903397350438498ee68f2566ae06e060fa51c15c93aba705,
      46,052 bytes.
references:
- https://classicmacdemos.com/prince-of-persia
- https://static.classicmacdemos.com/demos/prince-of-persia/README.txt
- https://www.jordanmechner.com/en/games-movies/prince-of-persia/
- https://www.ubisoft.com/en-us/game/prince-of-persia
---

## Sixty minutes, one dungeon

![Prince of Persia gameplay](incoming/prince-of-persia/gameplay.png)

The demo opens with the palace title sequence, then drops the Prince into the
first underground chamber. Movement has weight: a running start carries him
across a gap, a careful step avoids a loose tile, and reaching a ledge is only
half the task because he still has to catch it. Torches, gates, pressure plates
and concealed passages turn the stonework into a chain of physical puzzles.

This Macintosh conversion preserves the rotoscoped animation that made the
original distinctive. The Prince accelerates into a run, braces on landing and
pulls himself over ledges one deliberate motion at a time. The demonstration
includes colour, LC and black-and-white data so the same package could adapt to
several contemporary Macintosh displays.

## The original promotional package

Systemless opens Brøderbund's StuffIt archive directly, discovers the demo
application and its matching data files, and reaches live Level 1 gameplay.
The current deterministic run exercised sustained movement through the first
obstacle and matched the BasiliskII oracle at 99.8% overall perceptual parity.

The package identifies itself as a demo and includes Brøderbund's original
ordering information. It is preserved byte-for-byte rather than replaced with
the retail game, a cracked copy or a browser remake. The launcher remains
disabled until the archive and screenshot complete asset promotion and browser
review.
