---
id: spin-doctor-demo
kind: game
title: Spin Doctor Demo
summary: Swing a spinning wand across a grid of nodes in Callisto's puzzle game.
developer: Callisto Corporation
publisher: Callisto Corporation
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, entered a doctor name, and reached the
      live Level 1 Grid board. Matched-tick replays with Space and Command input changed
      the wand pose against the no-input baseline.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3644
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + release browser at 10 MHz
    architecture: 68k
    environment: >-
      Fetched the exact archive once in a release-mode browser, entered a name,
      started Grid, and observed its live rotating wand. At 10 MHz a five-second active
      sample measured 60.0 host FPS and 60.0 guest ticks/sec, with a 9.8 ms maximum
      measured frame and 120 ms minimum audio queue. The 518-by-386 screenshot is a direct
      capture of the game board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3644
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: ea9e96aaf4295dd632c6435a1d672d85980e802352fabe10eb3332758013425a
    size_bytes: 273811
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://download.classicmacdemos.com/Spin%20Doctor%20Demo.sit
    rights_holder: Callisto Corporation
    permission: >-
      The unchanged archive contains only the purpose-built Spin Doctor Demo
      application and no bundled redistribution restriction. Contemporary 1994 shareware and
      cover-disc records identify the demo; no retail files are included.
    notes: >-
      Original 273,811-byte StuffIt archive, SHA-256
      ea9e96aaf4295dd632c6435a1d672d85980e802352fabe10eb3332758013425a.
      The promoted public archive and screenshot were fetched back and matched their
      pinned SHA-256 hashes and byte lengths.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2f3642461572043c6f7ae0b98771b9b617651e1c6203a3137bf6987fc07b653a
    size_bytes: 9063
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3644
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
    notes: >-
      Direct 518-by-386 release-browser capture of the Grid board, excluding website
      framing and the Mac menu bar.
references:
- https://groups.google.com/g/comp.sys.mac.games/c/JyAhTlLNCgA
- https://www.macintoshrepository.org/16255--the-mac-magazine-cover-floppy-disks
---

## Swing to the goal

![Spin Doctor Level 1 Grid board](https://assets.systemless.org/catalogue/media/sha256/2f/2f3642461572043c6f7ae0b98771b9b617651e1c6203a3137bf6987fc07b653a.png)

Enter a doctor name and start the Grid level. You are the white spinning wand:
press Command to swing to a neighboring node, Option to bounce, and Space to
reverse direction. Reach the rotating goal dot while avoiding obstacles.
