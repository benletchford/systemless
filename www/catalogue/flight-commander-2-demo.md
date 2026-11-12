---
id: flight-commander-2-demo
kind: game
title: Flight Commander 2 Demo
summary: Plan aircraft maneuvers in the original Avalon Hill tactical air-combat demo.
developer: Big Time Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: '2026-11-13'
    tester: Catalogue maintainer
    systemless_version: ab6ff5f3 native candidate
    architecture: 68k
    environment: Complete original seven-file period demo, default classic machine and preferred partition,
      original American-human/novice-computer defaults. Generic native-trap watch-target cache candidate
      in PR6226; no game files or settings changed. Normal Open Battle and Demo Battle selection reaches
      the real mission briefing and battlefield. Paired runs both complete 2568 frontend / 3168 guest
      ticks with zero exhausted frames and identical pre-input RGB. Only active applies a left steering
      press; stick, High-G indicator and planned path differ by1658pixels, then ongoing Action simulation
      puts its aircraft at a different position (957pixels within x432-472/y336-366). Both actual final
      captures show Turn1/Action and missile/ECM events. Bounded battle entry, planning input and ongoing
      simulation only; completed turn/mission, full combat, audio fidelity, hardware accuracy and browser
      pacing remain unverified. All actual captures inspected. Native source is the tested candidate,
      not a public release.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6225
runtime:
  executable_path: FC2/Flight Commander 2 Demo
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: incoming
    path: catalogue/incoming/flight-commander-2-demo/period-demo.zip
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://archive.org/details/IMG24JanFeb1995
    - https://github.com/benletchford/systemless/issues/6225
    license: Maintainer-accepted promotional demo
    rights_holder: Big Time Software and original contributors
    permission: Accepted under the user-authorized existing project policy for complete unchanged period
      promotional demos. This records project-policy acceptance, not an affirmative publisher or artwork
      licence grant. No explicit restriction on intact game-demo sharing was identified in the reviewed
      notices. Retain all original files, notices and demo limits.
    notes: New preservation ZIP of the entire original Demos/FC2 folder on Inside Mac Games Vol3 Issue1&2,
      Jan/Feb1995. Wrapper is synthesized, not publisher-issued. The original ISO size683059200 and published
      SHA1e8c1dfe89923172ba3f07041227066f575ac83bc are verified. All7logicalfiles,14rawforks and original
      type/creator/flag records match after real production packing, with strict EOF; filenames and icon
      coordinates are also preserved. AppleDouble metadata wrappers are synthesized; unavailable extended
      Finder fields are not claimed preserved. No original fork modified, replaced or reconstructed. The
      separate eight-file mirror includes an extra auto-save file and was not trimmed or used for this
      export. Original7021byte README matches the published copy; full period root ReadMe and1339standard
      UI/string/dialog/version resources reviewed, zero parse errors; only the1.01demo copyright/version
      notice identified. No guest instruction, game-script or opaque-asset decoding.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/flight-commander-2-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6225
    rights_holder: Big Time Software and original contributors
    permission: Actual content-only capture of the project-policy-accepted original demo; no separate
      artwork licence grant claimed.
    notes: Unscaled unretouched800x580lossless pixel crop from native battle gameplay after a left steering
      order; only the OS menu bar removed. Flight controls, aircraft and changed planned flight path visible.
references:
- https://archive.org/details/IMG24JanFeb1995
- https://github.com/benletchford/systemless/issues/6225
- https://github.com/benletchford/systemless/pull/6226
- https://classicmacdemos.com/flight-commander-2
---

Choose **Open Battle**, then **Demo Battle**, and keep the original player defaults. Dismiss the briefing and navigator report. Use the flight stick to plan a maneuver, then choose **Action** to run the orders.

This is the complete seven-file period demo in a preservation ZIP.
