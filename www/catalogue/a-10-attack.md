---
id: a-10-attack
kind: game
title: A-10 Attack!
summary: Fly Parsoft's Warthog in its original five-minute Macintosh demo.
developer: Parsoft Interactive
publisher: Parsoft Interactive
year: 1995
architectures: [68k]
default_architecture: 68k
category: Simulation
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: '2026-09-23'
    tester: Catalogue maintainer
    systemless_version: 0.52.0-dev
    architecture: 68k
    environment: >-
      Deterministic headless play of the unchanged 1.1 demo at 800-by-600 in
      256 colours. Selected Quick Start and the Fly A-10 mission, then reached
      the live cockpit and runway view.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2482
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://gardenmirror.oldapplestuff.com/games/A-10_Attack_DEMO.sit
    expected_sha256: 090cf836df3f6971538cf7fd7db64f29a4e368b95d791d0a1ed8b209f7640a3a
    expected_size: 1341067
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/a-10-attack
    - https://gamefaqs.gamespot.com/mac/564848-a-10-attack/faqs/2594
    rights_holder: Parsoft Interactive
    permission: >-
      This unchanged, five-minute-per-mission Macintosh demonstration was
      distributed publicly to promote the retail game. The bundled Read Me
      identifies it as a demo and advertises the full release. No time-limit
      removal patch or commercial edition is included. The archive has no
      express redistribution clause.
    notes: >-
      Original 1,341,067-byte StuffIt archive, SHA-256
      090cf836df3f6971538cf7fd7db64f29a4e368b95d791d0a1ed8b209f7640a3a.
      MD5 b14b2d73ce07c4d0e6d61e88d0846637 matches Macintosh Garden's
      A-10_Attack_DEMO.sit listing. Its Read Me describes two Quick Start
      missions on a smaller demo island.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/a-10-attack/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2482
    permission: >-
      Fresh deterministic gameplay capture made for this catalogue entry.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Captured the 640-by-480 flight-content region at (80,60) after selecting
      the Fly A-10 Quick Start mission. The crop excludes the Mac menu bar and
      desktop without altering game pixels. PNG SHA-256
      f25aa24d039355c1dadb550ae5f82327b46cf08e64a0a702847e42b623c138bf,
      222,530 bytes.
references:
- https://macintoshgarden.org/games/a-10-attack
- https://gamefaqs.gamespot.com/mac/564848-a-10-attack/faqs/2594
---

## In the cockpit

![A-10 Attack! demo cockpit and runway](incoming/a-10-attack/gameplay.png)

Parsoft's flight simulation starts with two Quick Start missions: an unopposed
flight over Demo Island and a combat scenario. In the cockpit you can start the
engines, manage the aircraft's systems, and take off from the island runway.

## The original timed demo

This is the original Macintosh demonstration, not the full commercial game.
The bundled Read Me limits each mission to five minutes and describes the two
demo missions. The catalogue does not include a time-limit-removal patch.
