---
id: prince-of-persia-2
kind: game
title: "Prince of Persia 2: The Shadow and the Flame"
summary: Escape the palace rooftops in Brøderbund's playable Macintosh demo.
developer: Brøderbund Software
publisher: Brøderbund Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.56.0"
    architecture: 68k
    environment: "Deterministic run of the unchanged 68K demo with the generic 24-bit trap gateway correction in PR #2617. The application opens Prince2.opt, reaches its self-running sequence, and enters the first playable rooftop level after the publisher-documented mouse click. Holding F moves the Prince to the right. A native 512-by-384 capture shows the level without host or emulator framing. An optimized browser pacing probe reached 60 host FPS but only about 29.5 guest ticks per second, below the site's 50-tick launch gate (issue #2620), so browser launch remains disabled."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2618
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 20698e667cbbd30df04c88bc7224a1d8f84b67849c79ed7935ba5ec0552e37d1
    size_bytes: 1077354
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/prince-of-persia-2-the-shadow-the-flame
    - >-
      https://static.classicmacdemos.com/demos/prince-of-persia-2-the-shadow-the-flame/README.txt
    rights_holder: Jordan Mechner, Brøderbund Software, Ubisoft Entertainment and successors
    permission: >-
      Brøderbund deliberately distributed this limited Macintosh promotional demo.
      Its included Read Me identifies a self-running demonstration and a playable first
      level, gives controls and ordering information, and retains the original copyright
      notice. This supports preservation of the exact unchanged demo, not the retail
      game, repacks, or a broader redistribution licence; no express redistribution
      licence was found in the archive.
    notes: >-
      The unchanged 1,077,354-byte StuffIt archive has SHA-256
      20698e667cbbd30df04c88bc7224a1d8f84b67849c79ed7935ba5ec0552e37d1. It contains the Prince 2 Demo
      application, Data/Prince2.opt, and the publisher's Read Me First! The publisher's
      document dates this Mac demo campaign to 1994.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: aecf2e5a9778acfd8a2f706d10b9a78bbcba02c9a9de4e0bd6c28f487ad9458c
    size_bytes: 237471
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2618
    permission: >-
      Fresh gameplay capture made for this catalogue from the unchanged demo.
      Underlying game artwork remains the property of its rights holders.
    notes: >-
      Captured at guest tick 1142 after entering the first rooftop level. The display
      profile was set to the game's native 512-by-384 content size, so the captured
      PNG contains only game pixels without cropping or resampling. PNG SHA-256
      aecf2e5a9778acfd8a2f706d10b9a78bbcba02c9a9de4e0bd6c28f487ad9458c, 237,471 bytes.
references:
- https://classicmacdemos.com/prince-of-persia-2-the-shadow-the-flame
- >-
  https://static.classicmacdemos.com/demos/prince-of-persia-2-the-shadow-the-flame/README.txt
- https://github.com/benletchford/systemless/issues/2614
- https://github.com/benletchford/systemless/issues/2618
- https://github.com/benletchford/systemless/issues/2620
---

## The first rooftop escape

![Prince of Persia 2 demo rooftop gameplay](https://assets.systemless.org/catalogue/media/sha256/ae/aecf2e5a9778acfd8a2f706d10b9a78bbcba02c9a9de4e0bd6c28f487ad9458c.png)

Brøderbund's original Macintosh demonstration offers the opening rooftop level
of *The Shadow and the Flame*. The Prince is chased from the palace across
sloping roofs and past armed guards. The package also includes a self-running
sequence, but it is a limited demo rather than the complete game.

Click the game window or press Escape during the demonstration to start the
playable first level. Run left with 4, S, or J, and right with 6, F, or L; 8,
E, or I jumps or climbs. Hold Shift or Control to grab a ledge. The archive's
Read Me gives the full movement and sword controls.

Systemless reaches the rooftop and responds to movement using the unchanged
demo archive. Browser launch remains disabled while its current guest-tick
performance is investigated.
