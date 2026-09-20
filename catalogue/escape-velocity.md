---
id: escape-velocity
kind: game
title: Escape Velocity
summary: Trade cargo, explore star systems and choose sides in a spacefaring conflict.
developer: Matt Burch
publisher: Ambrosia Software
year: 1996
architectures:
- 68k
- ppc
default_architecture: 68k
category: Space Trading
aliases:
- /ev
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-13
    tester: Catalogue maintainer
    systemless_version: 0.40.0
    architecture: 68k
    environment: Local website preview in the in-app browser
    status: playable
    evidence: >-
      https://github.com/benletchford/systemless.org/blob/master/entries/escape-velocity.md
controls:
  mobile:
    button_groups:
    - buttons:
      - key: Space
        label: Fire
      - key: Shift
        label: Sec
      - key: W
        label: Weap
      - key: Tab
        label: Targ
      - key: R
        label: Near
      - key: S
        label: Safe
      label: Wpn
    - buttons:
      - key: Z
        label: Burn
      - key: A
        label: Auto
      - key: J
        label: Jump
      - key: H
        label: Hyp
      - key: \
        label: HSel
      - key: L
        label: Land
      label: Nav
    - buttons:
      - key: Enter
        label: OK
      - key: "Y"
        label: Comm
      - key: B
        label: Brd
      - key: C
        label: Recl
      - key: V
        label: Hold
      - key: F
        label: Retg
      label: Dock
    - buttons:
      - key: M
        label: Map
      - key: P
        label: PInf
      - key: I
        label: MInf
      - key: U
        label: Clok
      - key: Escape
        label: Paus
      label: Info
    enabled: true
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 5c6b8ed5ee1efb67f3f7cbe62b051f36d26753337f18a238febbdc023ef0a5a2
    size_bytes: 5401984
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/escape-velocity
    - https://download.macintoshgarden.org/games/EV_Installer_1.0.5.bin
    license: Ambrosia Software shareware license
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      Non-profit redistribution of the complete, unmodified package is permitted.
      Distribution for profit requires written permission.
    notes: "Checked the bundled Escape Velocity License.text in the independently retrieved 1.0.5 installation. Registration is required after the trial period. Retrieved 2026-09-13: 5401984 bytes; SHA-256 5c6b8ed5ee1efb67f3f7cbe62b051f36d26753337f18a238febbdc023ef0a5a2."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 5294854e83df7be34719c02df8ffc3f6f4a9b48f1b6389b4afeb92fa4f39bfd0
    size_bytes: 25182
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/tree/master/entries
    permission: >-
      Original screenshot captured for this catalogue at the maintainer’s request.
      Underlying game artwork remains the property of its respective rights holders.
    notes: >-
      Unedited 800×600 game framebuffer captured headlessly on 2026-09-15 while
      replaying a fresh pilot from the untouched 1.0.5 MacBinary installer. No browser,
      operating-system frame, or catalogue controls are present.
references:
- https://macintoshgarden.org/games/escape-velocity
---

## Source and version

This is the untouched [Escape Velocity 1.0.5 installer](https://assets.systemless.org/catalogue/objects/sha256/5c/5c6b8ed5ee1efb67f3f7cbe62b051f36d26753337f18a238febbdc023ef0a5a2.bin), preserved in its original MacBinary form. Systemless opens the installer and finds the game inside it at launch; the catalogue does not keep a second, repacked copy.

## Plug-ins

The collection below is a doorway into the astonishing amount of work EV players made for one another: new ships, mission arcs, utilities and full conversions. Each download is the original package held by the source archive. Automatic installation is deliberately unavailable until Systemless can understand those packages without rebuilding them.

## Gameplay

![Escape Velocity gameplay](https://assets.systemless.org/catalogue/media/sha256/52/5294854e83df7be34719c02df8ffc3f6f4a9b48f1b6389b4afeb92fa4f39bfd0.png)

## The life of a pilot

You begin with a shuttle, ten thousand credits and no prescribed career. Trade routes offer the gentlest start, but courier work, bounty hunting, piracy and the growing war between the Confederation and Rebellion soon pull the same little ship in very different directions. Bar conversations matter: many of the game's best stories begin as an unassuming job on an ordinary world.

Use the arrow keys to steer, **L** to select and land, **M** for the map, and **J** to jump once you are clear of the system centre.
