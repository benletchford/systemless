---
id: hoyle-board-games-demo
kind: game
title: Hoyle Board Games Demo
summary: >-
  Match exposed tiles in Zen Bones and explore Sierra's original Macintosh
  board-game collection demo.
developer: Sierra On-Line
publisher: Sierra On-Line
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Puzzle
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: f8c23613 native-equivalent runtime
    architecture: ppc
    environment: >-
      Complete unchanged demo with original guest defaults. Open Zen Bones,
      preserve the original Classic layout selection and match two exposed
      seven-bamboo tiles. The active run removes them; the omitted-input control
      retains them. Both pass 12 pixel assertions at 661 frontend and guest ticks
      with zero exhausted frames. RGB-normalized pre-input captures match exactly;
      post-input captures differ at 3903 pixels. Actual captures inspected.
      This verifies one bounded native matching action. Checkers displays the
      original full-version-only message and remains unavailable. Other games,
      puzzle completion, persistence, audio, browser pacing and independent hardware
      accuracy remain unverified. Frozen binary adcaa1a4 uses source 01638c43
      with the runtime tree merged in f8c23613.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6063
runtime:
  executable_path: Hoyle Board Games Demo/Hoyle Board Games Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Hoyle%20Board%20Games%20Demo.sit
    expected_sha256: 9586be59df8b263d1a9be80c0f66caf27c5fa613ed08bf12041a366314e69449
    expected_size: 14219481
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/hoyle-board-games
    - https://archive.org/details/macworld-uk-cd-august-1999
    - https://github.com/benletchford/systemless/issues/6063
    license: Maintainer-accepted promotional demo
    rights_holder: Sierra On-Line and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete,
      unchanged, period-distributed promotional demos. This records project-policy
      acceptance without claiming an affirmative publisher licence grant. No
      applicable explicit redistribution restriction was identified in reviewed
      notices. Preserve the complete archive, documentation, notices and demo limits.
    notes: >-
      All 27 core file paths and data forks and all 79 resource payloads, names,
      IDs and attributes match the August 1999 Macworld UK disc installer. File
      types and creators match; Finder flags and two raw resource-map bookkeeping
      states differ. The received SIT wrapper is preserved unchanged and is not
      claimed period-byte-identical. Independent unar extraction matches production
      packing on 27 files, 54 forks and 27 Finder records. The complete 70-page
      PDF help text was extracted and notice-term matches reviewed; the image-only
      cover and final page were inspected. The linked mirror README is byte-identical
      to bundled help. All 42 selected received UI/text/version records and 61
      period installer records were reviewed; none of these selected records is compressed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/hoyle-board-games-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6063
    rights_holder: Sierra On-Line and original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted demo.
      Underlying artwork retains its original copyright.
    notes: >-
      Unscaled 640x480 game content cropped from the verified native matching run.
      Excludes the Mac menu bar, desktop and window framing.
references:
- https://classicmacdemos.com/hoyle-board-games
- https://github.com/benletchford/systemless/issues/6063
---

![A matching pair removed from Zen Bones in Hoyle Board Games](incoming/hoyle-board-games-demo/gameplay.png)

Open Zen Bones and find matching exposed tiles. A tile is exposed when it has no
tile above it and has a free left or right edge. Seasons can match other seasons;
flowers can match other flowers.

The original demo limits are preserved. Native matching is verified; browser
qualification is pending.
