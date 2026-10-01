---
id: pga-tour-golf-ii-demo
kind: game
title: PGA TOUR Golf II Demo
summary: Practice drives at TPC at Sawgrass in EA's 1994 golf demo.
developer: Polygames / Looking Glass Technologies
publisher: Electronic Arts
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo. Opened the Go menu, selected the TPC at
      Sawgrass driving range, and clicked Drive. The golfer changed from address to a
      swing and the power meter advanced.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3695
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.71.0 + release-mode browser build
    architecture: 68k
    environment: >-
      Chrome fetched the unchanged archive once, opened the TPC at Sawgrass driving
      range, and clicked Drive. The golfer swung and the power meter changed. A
      five-second active-range sample measured 60.0 host frames/s, 60.2 guest ticks/s, an 11.5
      ms maximum frame, and a 131 ms minimum audio queue.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3695
runtime:
  runtime_pacing:
    cpu_mhz: 10
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 08ea43250e91c896dc71e803493cb837186ccb9bfcca35c90fb4918110d2636b
    size_bytes: 1259456
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/pga-tour-golf-ii
    rights_holder: Electronic Arts / PGA TOUR
    permission: >-
      The unchanged archive is labeled PGA TOUR Golf II Demo and contains the demo
      application, TPC at Sawgrass course, graphics, and pro stats. No bundled
      redistribution restriction was found; no retail application is included.
    notes: >-
      Original 1,259,456-byte StuffIt archive, SHA-256
      08ea43250e91c896dc71e803493cb837186ccb9bfcca35c90fb4918110d2636b.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: fa0bfd95e88a389d364db39baa13486eac618c170e45ffd6ff7f800e276f705b
    size_bytes: 36607
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3695
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
    notes: 640-by-480 direct Chrome capture of the active driving-range swing.
references:
- https://classicmacdemos.com/pga-tour-golf-ii
---

## Practice a drive

![PGA TOUR Golf II driving range](https://assets.systemless.org/catalogue/media/sha256/fa/fa0bfd95e88a389d364db39baa13486eac618c170e45ffd6ff7f800e276f705b.png)

Open Go and choose Driving Range, then select TPC at Sawgrass and click Play.
Choose a club and click Drive to swing. The demo also offers putting-green
practice and a practice round.
