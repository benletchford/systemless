---
id: escape-velocity-nova
kind: game
title: Escape Velocity Nova
summary: >-
  Trade, explore, and fight across branching spacefaring storylines in the third
  Escape Velocity game.
developer: ATMOS and Matt Burch
publisher: Ambrosia Software
year: 2002
architectures:
- ppc
default_architecture: ppc
category: Space Trading
aliases:
- /evn
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-15
    tester: Catalogue maintainer
    systemless_version: 0.41.1
    architecture: ppc
    environment: Local website preview in the in-app browser using the published crate
    status: playable
    evidence: https://github.com/benletchford/systemless.org/pull/99
runtime:
  worker: true
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
  format: sit
  source:
    type: sha256
    sha256: 7211ea030bf9dd1a06ed913857281050e514ee03a25b6c918bef5774d6d39c61
    size_bytes: 80510398
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://tidbits.com/2002/03/11/the-plain-truth-about-casual-software-piracy/
    - https://macintoshgarden.org/games/escape-velocity-nova
    - https://download.macintoshgarden.org/games/EV_Nova_1.0.8.sit
    license: Proprietary shareware (30-day trial)
    rights_holder: Ambrosia Software, ATMOS and Matt Burch
    permission: "Noncommercial sharing of the original unregistered shareware package, based on Ambrosia's published sharing policy: Matt Slot's publisher-authored article encourages making and sharing copies of its shareware. The bundled manual identifies this exact release as a 30-day trial. The archive is unchanged; no registration codes or patches are included."
    notes: "Original unregistered EV Nova 1.0.8 shareware distribution. The bundled EV Nova Documentation.pdf describes the trial and registration restrictions (page 37). Retrieved 2026-09-13: 80510398 bytes; SHA-256 7211ea030bf9dd1a06ed913857281050e514ee03a25b6c918bef5774d6d39c61."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/escape-velocity-nova/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/tree/master/entries
    permission: >-
      Original screenshot captured for this catalogue at the maintainer’s request.
      Underlying game artwork remains the property of its respective rights holders.
    notes: >-
      Unedited 800×600 game framebuffer captured headlessly on 2026-09-21 from the
      untouched EV Nova 1.0.8 shareware archive using the v0.45.0 browser runtime at its
      deterministic 1400-tick main-menu checkpoint. It shows the PowerPC main menu with
      no browser, operating-system frame, or catalogue controls.
references:
- https://macintoshgarden.org/games/escape-velocity-nova
---

## Source and version

This entry keeps the original, unregistered [EV Nova 1.0.8 shareware archive](https://assets.systemless.org/catalogue/objects/sha256/72/7211ea030bf9dd1a06ed913857281050e514ee03a25b6c918bef5774d6d39c61.sit) intact. The bundled manual describes a 30-day trial, and none of its registration restrictions have been removed.

## Gameplay

![Escape Velocity Nova gameplay](incoming/escape-velocity-nova/gameplay.png)

## A larger galaxy

Nova gives the series' familiar small-ship freedom a grander, stranger setting. Freight and passenger work can keep you independent for a while, but the Federation, Aurorans, Polaris and Vell-os each offer a genuinely different road through the galaxy. Ships are not merely bigger health bars here: their bays, outfit space and weapon arcs invite distinct ways to fly.
