---
id: space-invaders-1-1
kind: game
title: Space Invaders 1.1 ƒ
summary: >-
  Defend the Earth from descending invaders in Simone Bettini's colourful
  classic arcade shooter.
developer: Simone Bettini
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: Deterministic gameplay run from the complete unchanged Space Invaders 1.1 ƒ archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2322
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Space%20Invaders%201.1%20%C6%92.sit
    download_page: https://www.vintageapplemac.com/software/games/s/
    expected_sha256: c052d868750e3083684eb0f5248c9e7c07ac399787f5d65195cfbb2392140794
    expected_size: 198535
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/s/
    - https://www.vintageapplemac.com/files/games/Space%20Invaders%201.1%20%C6%92.sit
    license: Space Invaders ReadMe §5 Diffusion
    rights_holder: Simone Bettini
    permission: >-
      The bundled ReadMe §5 states: “The game can be included together with this
      readme file in any shareware-freeware collection. I'd just like to be
      advised of it and, if possible to receive a copy of the collection
      (floppy or CD).” The game and ReadMe are retained together and unchanged;
      the same section describes the game as Shareware and requests a $10 (or
      greater) fee, or a postcard or note when payment is not possible.
    notes: >-
      Unchanged 198,535-byte StuffIt archive, pinned by SHA-256
      c052d868750e3083684eb0f5248c9e7c07ac399787f5d65195cfbb2392140794. The
      complete package retains the 68k application, ReadMe and Icon.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/space-invaders-1-1/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2322
    permission: >-
      Original screenshot captured from the exact Space Invaders 1.1 ƒ archive
      during a deterministic gameplay run for this catalogue. Underlying game
      artwork remains the property of its rights holder.
    notes: >-
      Fresh capture during live Wave 1 with invaders and projectiles visible,
      cropped to the 512-by-299 game content surface. It excludes the host
      desktop, emulator margins and Classic Mac menu bar.
references:
- https://www.vintageapplemac.com/software/games/s/
- https://www.vintageapplemac.com/files/games/Space%20Invaders%201.1%20%C6%92.sit
---

## Defend the Earth

Space Invaders 1.1 ƒ is Simone Bettini's colourful 68k take on the classic
arcade shooter. Move the defender with the mouse, keep the laser firing and
survive the descending formations.

## Preserved with its collection permission

The bundled ReadMe describes Space Invaders as MixWare and explicitly permits
the game to be included with its ReadMe in any shareware-freeware collection.
It asks to be advised and, if possible, to receive a copy of the collection;
the unchanged archive keeps both the game and ReadMe together, along with its
Icon.

## In Wave 1

![Space Invaders gameplay](incoming/space-invaders-1-1/gameplay.png)

Systemless reaches a live Wave 1 from the exact archive, with the defender,
invaders and projectiles visible. A deterministic mouse-input run moved the
defender, producing distinct before and after frames and verifying gameplay
beyond launch and menu navigation.
