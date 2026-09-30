---
id: firefall-arcade-demo
kind: game
title: Firefall Arcade Demo
summary: Pilot a ship through Pangea Software's colorful arcade shooter.
developer: Pangea Software
publisher: Inline Software
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 347719a5 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo. Clicked through the
      splash, title, and high-score screens, held Play Normal briefly, and reached the
      active arcade board. Browser Right Arrow moved the ship right. One exact archive
      fetch. A 1.5-second active-board sample measured 60.0 host FPS and 60.0 guest
      ticks per second, maximum measured frame 4.9 ms, and minimum audio queue 482 ms. The
      demo round ends quickly and then displays its full-version promotion.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3603
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo at 800 by 600. Clicked through the splash
      and title screens, chose normal play, and reached the live playfield. The ship,
      moving enemies, score panel, and changing shot pattern remained visible after Left,
      Right, and Space inputs.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3603
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 9c76a1c5c666b31921fcf8171f9ae68d73d44a6943baaf687e8078d11f47afcc
    size_bytes: 494365
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/firefall-arcade
    - https://www.pangeasoft.net/firefall/oldindex.html
    rights_holder: Pangea Software / Inline Software
    permission: >-
      Pangea Software's official Firefall page says the game was released in 1993 and
      re-released as freeware. This unchanged archive contains the original 68K
      demonstration build, not a retail substitution. It has no bundled Read Me or additional
      redistribution restriction.
    notes: >-
      Unchanged 494,365-byte StuffIt archive, SHA-256
      9c76a1c5c666b31921fcf8171f9ae68d73d44a6943baaf687e8078d11f47afcc. The demo's own promotion credits Inline
      Software as its distributor.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ee34d58a9612420bbf60b298f6eea5eaca376e13001547c940b6440b99d0a6ac
    size_bytes: 113449
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3603
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      640-by-478 direct Chrome capture of the active arcade board. The crop excludes
      website framing and empty outer screen space. PNG SHA-256
      ee34d58a9612420bbf60b298f6eea5eaca376e13001547c940b6440b99d0a6ac, 113,449 bytes.
references:
- https://classicmacdemos.com/firefall-arcade
- https://www.pangeasoft.net/firefall/oldindex.html
---

## Hold the line

![Firefall Arcade playfield](https://assets.systemless.org/catalogue/media/sha256/ee/ee34d58a9612420bbf60b298f6eea5eaca376e13001547c940b6440b99d0a6ac.png)

Click through the splash and title screens, then choose a play mode. Use
the arrow keys to steer and Space to fire through the approaching waves.
