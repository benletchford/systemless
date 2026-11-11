---
id: king-of-dragon-pass-demo
kind: game
title: King of Dragon Pass Demo
summary: >-
  Guide a clan through one year in Glorantha in A Sharp's original Macintosh
  strategy and storytelling demo.
developer: A Sharp
publisher: A Sharp
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: f8c23613 native-equivalent runtime
    architecture: ppc
    environment: >-
      Complete unchanged demo with original guest defaults. Open the original demo
      document through the player's file chooser, choose Demo, proceed from the village
      overview and allocate one point of Sacred Time magic to Crops. The checkbox
      becomes selected and the reserve changes from seven to six; an omitted-click control
      retains seven. Both runs pass nine assertions at 690 frontend and guest ticks with
      zero exhausted frames. RGB-normalized pre-input frames match exactly; post-input
      frames differ at 80 pixels. Actual captures inspected. This verifies one bounded
      native choice, not a whole game year, browser pacing, audio or independent
      hardware accuracy. Frozen binary adcaa1a4 uses source 01638c43 with the runtime tree
      merged in f8c23613; the later sparse-gap optimization is not part of this native
      binary.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6067
runtime:
  executable_path: King of Dragon Pass/Player/King of Dragon Pass demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 3aba719d3e10237f6c1af959ff0dff92be08e2d4d3fb79000a62e44ea3d09436
    size_bytes: 36028757
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/king-of-dragon-pass
    - https://archive.org/details/macaddict-cd-044
    - https://github.com/benletchford/systemless/issues/6067
    license: Maintainer-accepted promotional demo
    rights_holder: A Sharp, LLC and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in reviewed notices. Preserve the complete
      archive, documentation, notices, bundled player and plugins, font files and
      original one-year demo limit.
    notes: >-
      All 16 core file paths and data and raw resource forks match the April 2000
      MacAddict 44 installer, including all 800 resource payloads, names, IDs and
      attributes. Types and creators match; four Finder initialized flags differ. The received
      SIT is preserved unchanged and is not claimed period-wrapper byte-identical.
      Independent unar extraction matches production packing on 16 files, 32 forks and 16
      Finder records. The included 1008-byte read-me and five-page PDF guide match their
      linked copies. Included notices and notice-term matches in 209 selected received
      UI/text/version records and 86 plain period-installer records were reviewed.
      Another 32 selected period-installer UI records are ADCR-compressed and remain
      unread by inspection tooling. The original installer was also visually inspected
      through its normal initial and destination-selection screens; no licensing dialog was
      observed in that sequence. This is a scoped notice review, not a claim that every
      installer path or compressed UI record was examined.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 32652ac0a8f245783bbbbe6f5fe8a5fb1261c4ddff8b1400990d4ca78dc448ac
    size_bytes: 441617
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6067
    rights_holder: A Sharp, LLC and original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted demo.
      Underlying artwork retains its original copyright.
    notes: >-
      Unscaled 640x480 game content cropped from the verified native allocation
      choice. Excludes the Classic Mac menu bar, desktop and emulator framing.
references:
- https://classicmacdemos.com/king-of-dragon-pass
- https://www.a-sharp.com/kodp/
- https://github.com/benletchford/systemless/issues/6067
---

![One point of magic allocated to Crops during Sacred Time](https://assets.systemless.org/catalogue/media/sha256/32/32652ac0a8f245783bbbbe6f5fe8a5fb1261c4ddff8b1400990d4ca78dc448ac.png)

In the file chooser, choose Desktop, open King of Dragon Pass, then open
King of Dragon Pass demo. Choose Demo on the title screen and Proceed from
the village overview to allocate your clan's magic.

The original demo lets you play one game year. A private browser test reaches the village, but starting the demo exceeds the
frame-time and audio-buffer limits. Browser launch remains disabled.
