---
id: crystal-crazy-demo
kind: game
title: Crystal Crazy Demo
summary: >-
  Collect crystals and dodge strange creatures in the original Crystal Quest
  sequel demo.
developer: Patrick Buckland and Alasdair Klyne
publisher: Casady & Greene, Inc.
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-11-28"
    tester: Catalogue maintainer
    systemless_version: "0.129.1"
    architecture: 68k
    environment: >-
      An isolated native headless replay of the original StuffIt archive at
      800 by 600 reached Wave 1, rendered the ship, crystals, score and timer,
      and registered a crystal pickup after mouse steering. The matched idle
      replay kept the score at zero. Full wave completion and browser pacing
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6795
runtime:
  executable_path: Crystal Crazy Demo/Crystal Crazy
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 0b9106bed154e91b79a6e3358676c7518e1b8509a909a3808e15549f2c2e6f45
    size_bytes: 1300059
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/crystal-crazy
    - https://archive.org/details/macformat-019
    - https://github.com/benletchford/systemless/issues/6071
    license: Maintainer-accepted promotional demo
    rights_holder: Casady & Greene, Inc. and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve the
      complete archive, original credits and notices, external object files, graphics,
      sounds and original demo limits.
    notes: >-
      All 61 file paths and data forks, all 2245 resource payloads, names, IDs and
      attributes, and all 61 Finder type, creator and flag records match the Inside Mac
      Games October 1994 copy on MacFormat 19, December 1994. All 61 raw resource forks
      differ in resource-map serialization. The received SIT remains unchanged;
      period-wrapper and whole-disc byte identity are not claimed. Independent unar extraction
      matches production packing on 61 files, 122 forks and 61 Finder records.
      Notice-term screening covered 174 selected included UI, text and version resources, with
      no standard compressed attributes. No selected TEXT resources were present. The
      original title credits and demo label were visually inspected; this is a scoped
      notice review, not a claim that every artwork resource was text-extracted.
references:
- https://classicmacdemos.com/crystal-crazy
- https://coverdiscs.com/disc/macformat-disc-19/
- https://github.com/benletchford/systemless/issues/6071
- https://github.com/benletchford/systemless/issues/6070
---

The complete original v1.0.4 demo preserves the sequel's graphics, sounds and
external object files.

In native 68k testing, Continue opens the demo title. Starting the game enters
Wave 1 with visible sprites and a readable score and timer. Steering with the
mouse collects crystals and increases the score. Browser launch remains disabled
pending browser qualification.
