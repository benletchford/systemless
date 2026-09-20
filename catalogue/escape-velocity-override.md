---
id: escape-velocity-override
kind: game
title: Escape Velocity Override
summary: >-
  Make a living among the Crescent's rival civilizations, then decide whose
  future you are willing to fight for.
developer: Matt Burch and Peter Cartwright
publisher: Ambrosia Software
year: 1998
architectures:
- 68k
- ppc
default_architecture: 68k
category: Space Trading
aliases:
- /evo
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-14
    tester: Catalogue maintainer
    systemless_version: 0.40.2
    architecture: 68k
    environment: Local website preview in the in-app browser
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/1838
  - date: 2026-09-15
    tester: Catalogue maintainer
    systemless_version: 0.41.2
    architecture: ppc
    environment: Headless systemless-play run from the original MacBinary installer
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/1975
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
    sha256: a51a55c5df6dbb1eab41d1840ffdcb47a9857190db510a37ac1a695277744932
    size_bytes: 7064960
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/escape-velocity-override
    - https://download.macintoshgarden.org/games/EV_Override_Installer_1.0.2.bin
    license: Ambrosia Software shareware license
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      Non-profit distribution of the complete, unmodified software is permitted;
      registration is required after the trial period.
    notes: "Verified the bundled EV Override License.text after installing the independently retrieved original 1.0.2 installer. Retrieved 2026-09-13: 7064960 bytes; SHA-256 a51a55c5df6dbb1eab41d1840ffdcb47a9857190db510a37ac1a695277744932."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c6d8cb51221937000fbed3c98cbc37e5c768fdd49d8671524c58b5475beac183
    size_bytes: 29355
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
      replaying a fresh pilot from the untouched 1.0.2 MacBinary installer. No browser,
      operating-system frame, or catalogue controls are present.
references:
- https://macintoshgarden.org/games/escape-velocity-override
---

## Source and version

This is the untouched [Escape Velocity Override 1.0.2 installer](https://assets.systemless.org/catalogue/objects/sha256/a5/a51a55c5df6dbb1eab41d1840ffdcb47a9857190db510a37ac1a695277744932.bin), preserved in its original MacBinary form. Systemless opens the installer and finds the game inside it at launch; nothing has been pre-installed or repacked for the web.

## Plug-ins

Override inspired an enormous library of ships, stories and total conversions. The collection below points to those original packages, exactly as their authors distributed them. Automatic installation is deliberately unavailable until Systemless can understand the packages without turning their contents into catalogue-made files.

## Gameplay

![Escape Velocity Override gameplay](https://assets.systemless.org/catalogue/media/sha256/c6/c6d8cb51221937000fbed3c98cbc37e5c768fdd49d8671524c58b5475beac183.png)

## Life in the Crescent

The Crescent is not simply a larger map for the first game. Human expansion has reached the Miranu and their neighbours while the long UE–Voinian war shapes the frontier. Trading and small jobs still pay for your first upgrades, but the loyalties you build through missions decide which ships, outfits and corners of the story open to you.

Use the arrow keys to steer, **L** to select and land, **M** for the map, and **J** to jump once clear of the system centre.
