---
id: absolute-zero-demo
kind: game
title: Absolute Zero Demo
summary: Defend Europa in the original Macintosh turret and flying combat demo.
developer: The Domain
publisher: Domark
year: 1995
architectures:
- ppc
default_architecture: ppc
category: Simulation
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: '2026-11-13'
    tester: Catalogue maintainer
    systemless_version: 3d874b12 native
    architecture: ppc
    environment: 'Complete unchanged sixty-file Demo 1.0.4, default machine, display, preferred partition,
      sound and settings. Original Start and three lower-right cheek-screen actions pass the briefing
      and default weapon preparation into the turret world. Fresh paired runs each finish 2680 frontend
      / 2679 guest ticks with zero exhausted frames and identical pre-input RGB. Only active holds default
      right-arrow for 30 ticks; stars and ground lines change view while the omitted-input view stays
      steady. RGB differences: 10047 pixels after input, 25581 after continued simulation, including animated
      scene differences. All six actual before/input/final captures inspected. Bounded turret entry and
      view movement only; flying mission, full combat, completion, audio fidelity, hardware accuracy and
      browser pacing unverified. Current master has not been replayed for this entry. Native evidence
      does not approve browser launch.'
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6240
runtime:
  executable_path: Absolute Zero Demo/Absolute Zero Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/absolute-zero/
    download_page: https://classicmacdemos.com/absolute-zero
    expected_sha256: 5d57e6e0d5578f1e45dde10a1c840261e6fea125a2cdd91260dd948dba1ce4af
    expected_size: 6384839
  provenance:
    redistribution: permitted
    original: true
    sources: &id001
    - https://classicmacdemos.com/absolute-zero
    - https://archive.org/details/macaddict-cd-002
    - https://github.com/benletchford/systemless/issues/6240
    license: Maintainer-accepted promotional demo
    rights_holder: Domark, The Domain and original contributors
    permission: Accepted under the existing user-authorized project policy for complete unchanged period
      promotional demos. This is project-policy acceptance, not an independently established publisher
      or artwork licence grant. No explicit restriction on intact demo sharing identified in reviewed
      notices. Preserve every original file, notice and demo limit.
    notes: All sixty received file paths, 120 raw data/resource forks, Finder type/creator/flags and 1713
      typed resources exactly match Sponsors/Eidos/Absolute Zero Demo on MacAddict Issue 002 (October
      1996). Complete 675516416-byte ISO matches published SHA1 4a9111aaa69a9de9005cc683148cc8381865e65c.
      Independent real production packing preserves all paths, forks and Finder records with strict EOF.
      Original 22639-byte Domark December 20, 1995 release notes reviewed, including demo scope, controls,
      known problems, credits and contacts. Ninety-one standard TEXT/STR#/DITL/vers records parsed without
      errors and screened for redistribution restrictions. No opaque game/script or instruction decoding.
      SIT wrapper not claimed publisher-issued; no files trimmed, modified or mixed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/absolute-zero-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6240
    rights_holder: Domark, The Domain and original contributors
    permission: Actual content-only gameplay capture under the accepted promotional-demo project policy;
      no separate artwork licence grant claimed.
    notes: Lossless unscaled unretouched 640x480 pixel crop of the actual original PowerPC turret mission
      after right-arrow input and continued simulation. Only unused black framebuffer removed; complete
      game viewport and HUD retained.
references: *id001
---

Choose the **Turret Mission Demo** and **Start**. Use the lower-right cheek screen to advance through the briefing and default weapon preparation into the mission. The original Read Me explains the interface.

Use the **arrow keys** to move the view and **Space** to fire. This is the complete original two-mission promotional demo; native verification covers turret entry and view movement.
