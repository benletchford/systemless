---
id: incredible-machine-3-demo
kind: game
title: The Incredible Machine 3.0 Demo
summary: Run playful chain reactions in Sierra's original Professor Tim's Incredible Machines demo.
developer: Jeff Tunnell Productions
publisher: Sierra On-Line, Inc.
year: 1995
architectures: [68k, ppc]
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: f8c23613 native-equivalent runtime
    architecture: 68k
    environment: >-
      Complete supplied archive and original guest defaults, using the genuine shipped
      68k slice. Original title click, Credits Done and default Guest sign-in reach the
      puzzle menu. Tutorial Puzzle 1, Balls, loads through the visible puzzle control.
      Actions > Run Puzzle starts physics: green ball and basketball leave their starting
      positions, while an omitted-mouse-button control remains static. Both runs pass
      eight assertions at 1404 frontend ticks and zero exhausted frames; active 1982
      guest ticks and control 2004 reflect the retained-menu guest-clock freeze.
      RGB-normalized pre-input images match exactly; post-input images differ at 9226
      pixels. Fixed eight-ball and floor invariants hold. Actual captures inspected.
      This verifies a bounded native simulation action, not solving the puzzle,
      completing the demo, browser pacing or audio, or independent hardware accuracy.
      Frozen binary adcaa1a4 uses source 01638c43 with the runtime tree merged in
      f8c23613; the later sparse-gap optimization is not part of this binary.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6074
runtime:
  executable_path: MacFormat 67/Prof. Tim's Incredible Machines
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Incredible%20Machines%203.0.sit
    expected_sha256: cee1c62ee2f9284f8d65f329b7c7f7a4c7771de7cfb6ba24fd8f731965784c23
    expected_size: 6677958
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/the-incredible-machine-version-30
    - https://coverdiscs.com/disc/macformat-disc-67/
    - https://github.com/benletchford/systemless/issues/6074
    license: Maintainer-accepted promotional demo
    rights_holder: Sierra On-Line, Inc. and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve the
      complete archive and supplied volume structure, language files, all three help
      applications, graphics, audio, original notices and demo limits.
    notes: >-
      The received SIT is unchanged but contains a mirror-prepared HFS disk image for
      the original CD paths, as disclosed by its source page; it is not claimed a
      period-original archive wrapper. All ten game/data/help paths and data forks,
      all 2021 resource payloads, names, IDs and attributes, and all ten Finder
      type/creator/flag records match MacFormat 67, September 1998. All ten raw resource
      forks differ in resource-map serialization. Production packing preserves the
      supplied MacFormat 67 volume and twelve files. All ten actual game/help files,
      twenty forks and ten Finder records match independent machfs extraction.
      Desktop DB/DF are retained but were not independently exported by machfs; no
      all-twelve-file or whole-disc byte identity is claimed. Notice screening covered
      277 selected included UI/text/version resources without standard compressed
      attributes. Included TEXT, notice-term matches and the visible original title
      copyright were reviewed; not every artwork resource was text-extracted.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/incredible-machine-3-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6074
    rights_holder: Sierra On-Line, Inc. and original contributors
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 518x338 crop of the actual native playfield after Run Puzzle. Excludes
      the Classic Mac menu bar, desktop, window titles and emulator framing. Original
      artwork retains its copyright.
references:
- https://classicmacdemos.com/the-incredible-machine-version-30
- https://coverdiscs.com/disc/macformat-disc-67/
- https://github.com/benletchford/systemless/issues/6074
---

![Balls moving in Tutorial Puzzle 1 after Run Puzzle](incoming/incredible-machine-3-demo/gameplay.png)

Also titled Professor Tim's Incredible Machines. Click the opening screen, choose
Done on the credits, wait for the parts to load, and keep the default Guest sign-in.
Choose the visible puzzle control, dismiss the tutorial heading and select
Actions > Run Puzzle to start the simulation.

The original demo and its English, French and German help applications are retained.
Browser qualification is pending; launch remains disabled.
