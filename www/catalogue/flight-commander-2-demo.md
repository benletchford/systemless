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
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: 3d874b12 native
    architecture: 68k
    environment: "Complete original eight-file demo, default classic machine and preferred partition, original American-human/novice-computer defaults. Exact merged runtime 3d874b12; no game files or settings changed. Normal Open Battle and Demo Battle selection reaches the mission briefing and battlefield. Paired fresh-archive runs both complete 2568 frontend / 3168 guest ticks with zero exhausted frames and identical pre-input RGB. Only active applies a left steering press: stick, High-G indicator and planned path differ by 1658 pixels, then ongoing Action simulation puts its aircraft at a different position (957 pixels). Both final captures show Turn 1 / Action and missile/ECM events. Actual before/order/final captures inspected. Bounded battle entry, planning input and ongoing simulation only; completed turn/mission, full combat, audio fidelity, hardware accuracy and browser pacing remain unverified. Native verification does not approve browser launch."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6225
runtime:
  executable_path: Flight Commander 2 Demo/Flight Commander 2 Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: bb99fa70de5c8506e35b7827e2545f66e5349aad0611d2427715832d77c904cd
    size_bytes: 2460543
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/flight-commander-2
    - https://archive.org/details/mac-magazin-and-mac-easy-magazine-cd-issues-1-to-62
    - https://github.com/benletchford/systemless/issues/6225
    license: Maintainer-accepted promotional demo
    rights_holder: Big Time Software and original contributors
    permission: >-
      Accepted under the user-authorized existing project policy for complete
      unchanged period promotional demos. This records project-policy acceptance, not an
      affirmative publisher or artwork licence grant. No explicit restriction on intact
      game-demo sharing was identified in the reviewed notices. Retain all original files,
      notices and demo limits.
    notes: >-
      Complete unchanged eight-file received SIT. Every filename, all 16 raw
      data/resource forks and Finder type/creator/flags exactly match the Flight Commander 2
      Demo folder on Mac Magazin 10 (August 1995), including the original auto-save file.
      The full 671229952-byte period image matches published SHA-1
      1222414f8577043c67c10a4112039dbc66993111. The real production pack independently preserves all eight
      files, forks and Finder records exactly, with strict EOF. Archive wrapper is not
      claimed publisher-issued. Original 7052-byte READ ME, period root Bitte Lesen!
      and 1339 standard UI/string/dialog/version resources reviewed; zero parse errors.
      No explicit restriction on intact demo sharing identified. All original files,
      notices and demo limits retained. No guest instruction, game-script or opaque-asset
      decoding.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 15b914ed2fa8516030a441f7d8c253bbff25fb611d0f4f0b49b8c3173af43909
    size_bytes: 207054
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6225
    rights_holder: Big Time Software and original contributors
    permission: >-
      Actual content-only capture of the project-policy-accepted original demo; no
      separate artwork licence grant claimed.
    notes: >-
      Unscaled, unretouched 800x580 lossless crop from native battle gameplay after a
      left steering order on exact merged runtime 3d874b12 and the complete received
      eight-file archive. Only the OS menu bar removed. Flight controls, aircraft and
      changed planned flight path visible.
references:
- https://classicmacdemos.com/flight-commander-2
- https://archive.org/details/mac-magazin-and-mac-easy-magazine-cd-issues-1-to-62
- https://coverdiscs.com/disc/mac-magazin-10
- https://github.com/benletchford/systemless/issues/6225
- https://github.com/benletchford/systemless/pull/6226
---

Choose **Open Battle**, then **Demo Battle**, and keep the original player defaults. Dismiss the briefing and navigator report. Use the flight stick to plan a maneuver, then choose **Action** to run the orders.

This is the complete original eight-file promotional demo.
