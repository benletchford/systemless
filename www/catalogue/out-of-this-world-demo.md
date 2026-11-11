---
id: out-of-this-world-demo
kind: game
title: Out of This World / Another World Demo
summary: Escape the alien pool in MacPlay's original playable first-segment adventure demo.
developer: Delphine Software
publisher: MacPlay
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: 68k
    status: playable
    environment: >-
      Complete unchanged original SIT at default machine settings. Return opens
      the original window-size dialog; choose its default 640x400 option. Escape
      returns from the cinematic to Welcome; Return then starts the underwater
      opening. Holding the documented Up arrow for 180 frontend ticks brings the
      character to the surface and leaves him standing outside. A matched no-Up
      control stays underwater and dies. Pre-input RGB frames are identical.
      Both checked runs finish 1292 frontend and 1824 guest ticks, with zero
      exhausted frames and one observed scene-pixel assertion each. Actual
      response and settled captures inspected. No settings override, modified
      game files, passwords or unlocks. Browser input/pacing, the rest of the
      segment, audio fidelity and independent hardware accuracy remain unverified.
    evidence: https://github.com/benletchford/systemless/issues/6125
runtime:
  executable_path: Out of this World™ Demo 1.0.1/Out of this World™ Demo 1.0.1
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Out%20of%20this%20World%20Demo.sit
    expected_sha256: cb93f4ae64a70a1b00f7bab9e9ba1ce66bb16e2238f74302a605331d3f2a01c9
    expected_size: 754038
  provenance:
    redistribution: permitted
    original: true
    license: Maintainer-accepted promotional demo
    rights_holder: Delphine Software, MacPlay and original contributors
    sources:
    - https://classicmacdemos.com/out-of-this-world-another-world
    - https://coverdiscs.com/disc/inside-mac-games-vol-1
    - https://img.classicmacdemos.com/inside-mac-games-vol-1.iso
    - https://github.com/benletchford/systemless/issues/6125
    permission: >-
      Accepted under the existing project policy for complete, unchanged,
      period-distributed promotional demos, without claiming an affirmative
      publisher licence grant. No applicable explicit redistribution restriction
      was identified in the reviewed notices. Preserve the full original archive,
      files, descriptions, credits and first-segment demo restrictions.
    notes: >-
      Inside Mac Games Volume 1, February 1994, preserves this demo under More
      Goodies/More Demos/OOTW Demo. All 30 game-file data forks and 477 resource
      payloads, names, IDs and attributes match the independently extracted source.
      The archive's 401-byte Description and the disc's 432-byte dated Read Me
      are packaging variants; wrapper and description byte identity are not claimed.
      Both full texts and 40 readable selected standard UI/text/version records
      were reviewed, with zero compressed unread records. Original welcome,
      controls and window-size screens were inspected. Production packing matches
      all 31 files, 62 forks and 31 Finder records against independent unar.
      Opaque game data, scripts and guest instructions were not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/out-of-this-world-demo/screenshot.png
  provenance:
    redistribution: permitted
    content_only: true
    rights_holder: Delphine Software, MacPlay and original contributors
    sources:
    - https://github.com/benletchford/systemless/issues/6125
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 640x400 crop from actual native gameplay using the unchanged
      archive and the game's original default window-size option. Shows the
      character standing outside after escaping the pool. Excludes desktop,
      Classic Mac menu bar and outer framing; original artwork is unmodified.
references:
- https://classicmacdemos.com/out-of-this-world-another-world
- https://coverdiscs.com/disc/inside-mac-games-vol-1
- https://github.com/benletchford/systemless/issues/6125
---

## Out of the pool, into another world

This original Macintosh demo includes a playable first segment of Delphine's
cinematic adventure, also known as **Another World**.

Press **Return** at Welcome, then select the original default **640x400** size.
You can return from the cinematic to Welcome with **Escape** and press Return
again to enter play. Hold **Up** to swim out of the opening pool. The original
controls screen also describes directional movement and Shift, Control or
Option as action keys. Browser play remains pending.

![Out of This World demo gameplay](incoming/out-of-this-world-demo/screenshot.png)
