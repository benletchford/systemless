---
id: moria
kind: game
title: Moria
summary: Explore the dungeons in Purple X's colour Macintosh port of Moria.
developer: Richard Knuckey
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 1f9b99bc8afa3013ccbe7e99c84b460346bb58dc
    architecture: 68k
    environment: >-
      Bounded native replay of the complete unchanged Moria 1.1.1.cpt.sit archive
      through the checked Compact Pro loader. Default 800-by-600 display, 8-bit depth.
      Original File > New, Human/Male selection, statistics acceptance, Warrior selection
      and naming enter town; keypad 6, 8 and 4 move east, north and west. Inspected
      captures show town movement, descent to 50 feet and west/north dungeon movement.
      A fresh extended repeat passes 12 measured assertions at 4967 frontend ticks
      with zero exhausted budgets. Browser, public route,
      combat, sustained play, saves and audio remain unverified. No PPC slice exists
      in this original package.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4441
controls:
  arrows_as_numpad: true
runtime:
  executable_path: Moria 1.1.1.cpt/Moria
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://github.com/benletchford/systemless/releases/download/catalogue-intake-20261010/moria-111.cpt.sit
    expected_sha256: f835a419625bcc16ef2c91576562d20af62b33d0bb6c7d728139e9b5c2284725
    expected_size: 290684
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/m/
    - https://www.vintageapplemac.com/files/games/Moria%201.1.1.cpt.sit
    - https://github.com/benletchford/systemless/issues/4429
    - https://github.com/benletchford/systemless/issues/4441
    - https://github.com/benletchford/systemless/releases/tag/catalogue-intake-20261010
    license: Historical Moria nonprofit distribution notice
    rights_holder: Moria and UMoria contributors, including James E. Wilson, Robert A. Koeneke and Richard Knuckey
    permission: >-
      The original application's embedded TEXT 129 notice explicitly permits copying
      and distribution for educational, research and nonprofit purposes when its
      copyright and statement remain included. The original manual requires written
      permission for sales or for-profit distribution. All original notices are retained.
    notes: >-
      Complete unchanged six-file distribution with both original forks, including
      the game, top-level Read Me and four Documentation files. The outer StuffIt
      and inner Compact Pro are not repacked. Fresh original and intake downloads
      match the pinned hash and size. Version resources identify Purple X Moria
      1.1.1 / UMoria 5.5.2; all 12 CODE resources and no PPC cfrg identify this as 68K.
      The 1994 year is supported by original application creation/modification dates.
      This records the original historical grant, not an inferred modern GPL licence.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/moria/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4441
    permission: Original Systemless gameplay capture requested by the maintainer.
    notes: >-
      Inspected, unedited native 800-by-600 68K town gameplay frame after player
      movement. Contains only the guest desktop and game, with no host application UI.
references:
- https://www.vintageapplemac.com/software/games/m/
- https://groups.google.com/g/comp.sys.mac.games/c/U9N7fMkI16w
---

![Moria town gameplay](incoming/moria/gameplay.png)

Play the complete original **Purple X Moria 1.1.1** Macintosh port, based on
**UMoria 5.5.2**. Choose **New** from **File**. Pick a race, then **M** or **F**;
**Escape** accepts the rolled statistics. Pick a class, enter a name and press
**Return**, then **Return** again to enter town.

The **arrow keys** are mapped to the numeric keypad for movement. Keypad
**4**, **6**, **8** and **2** move west, east, north and south. On a down staircase,
**>** descends; **Space** acknowledges messages.

The complete original package and its copyright notices are preserved.
Redistribution is recorded under its original educational, research and
nonprofit terms.

Bounded native 68K testing covers character creation, town movement, descent
to 50 feet and movement in the dungeon. Browser and public-route testing,
combat, sustained play, saves and audio remain unverified. This original
package contains no PowerPC version.
