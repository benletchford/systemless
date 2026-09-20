---
id: space-junkie
kind: game
title: Space Junkie 1.2
summary: >-
  Pilot a tiny fighter through colorful Galaxian-inspired waves in Tuan Huynh's
  fast Macintosh arcade shooter.
developer: Tuan Huynh
publisher: Tuan Huynh
year: 1995
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
    environment: >-
      Deterministic headless gameplay run from the complete, unchanged Space
      Junkie 1.2 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2314
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Space%20Junkie%201.2.sit
    download_page: https://www.vintageapplemac.com/software/games/s/
    expected_sha256: c4d1e6c4ac930e213855f2cccb3d11c49a903975da065ef5ffc3381429f2c982
    expected_size: 97063
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/s/
    - https://www.vintageapplemac.com/files/games/Space%20Junkie%201.2.sit
    license: Space Junkie 1.2 non-commercial distribution permission
    rights_holder: Tuan Huynh
    permission: >-
      The bundled About Space Junkie 1.2 Distribution section states: “This
      program cannot be sold for a profit or distributed thru a commercial
      channel without my written consent. Distribution via online services,
      BBSs, Internet, person-to-person and other non-commercial means is
      acceptable provided that all original and unaltered files are included in
      the package.” This entry preserves the complete, unchanged archive.
    notes: >-
      Unchanged 97,063-byte StuffIt archive, pinned by SHA-256
      c4d1e6c4ac930e213855f2cccb3d11c49a903975da065ef5ffc3381429f2c982.
      The archive contains the 68k Space Junkie 1.2 application, About Space
      Junkie 1.2, Register Me, and the application icon. The bundled hardware
      requirements support 68020-or-later Macs and Power Macs; this entry
      declares the verified Systemless 68k path only.
references:
- https://www.vintageapplemac.com/software/games/s/
- https://www.vintageapplemac.com/files/games/Space%20Junkie%201.2.sit
---

## Release 1.2

Space Junkie is a colorful arcade shooter inspired by *Galaxian*. Tuan Huynh's
compact 1.2 release adds a larger 640-by-480 playfield, 256-color graphics,
digitized sounds, easier play, and a Power Macintosh-compatible build.

## Wave 1

The complete archive keeps the game, About document, registration application,
and icon together. A deterministic Systemless run dismissed the registration
screen, started New Game, entered live Wave 1, and processed held-left and
fire input. The HUD changed from `LINES 2` to `LINES 1` and the enemy formation
changed afterward, proving interaction beyond launch on the verified 68k path.

## Non-commercial distribution

The bundled Distribution section permits online-service, BBS, Internet,
person-to-person, and other non-commercial distribution only when all original
and unaltered files are included. It requires written consent for profit or
commercial-channel distribution; this catalogue entry preserves the complete
archive and does not grant commercial redistribution rights.
