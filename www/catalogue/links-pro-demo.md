---
id: links-pro-demo
kind: game
title: Links Pro Demo
summary: Play one Harbour Town hole in the original Macintosh Links Pro demo.
developer: Access Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Simulation
compatibility:
  status: unknown
  verified:
  - date: '2026-11-13'
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: 68k
    environment: Original unchanged demo at default machine, preferred partition, display, sound
      and preferences. File → New Game, add selected Jill Mac and accept original Pro level, Breezy
      weather and Normal greens. Hold Swing for 18 ticks, release for 18 ticks, then click again
      for two ticks. Active and omission runs begin with identical RGB captures and complete 2088
      frontend/2666 guest ticks with zero exhausted frames and zero script assertions. Actual captures
      show active backswing, follow-through and a 42-yard shot into water; omission remains on the
      tee with zero shots and 422 yards to the pin. The normal Drop → Hit From Here path returns
      to live play with the penalty recorded as two shots and 390 yards/Rough. No mulligan or settings
      change. Bounded native shot and normal hazard recovery only, not complete-hole, network, saves,
      audio-fidelity, hardware or browser-pacing qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6193
runtime:
  executable_path: Links Pro/Links Pro Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Links%20Pro.sit
    expected_sha256: 1bbf6d1b44d7504ced4d737e52de220a6c04b84a9c4c8de12028686604a24e97
    expected_size: 3221684
  provenance:
    redistribution: permitted
    original: true
    sources: &id001
    - https://classicmacdemos.com/links-pro
    - https://archive.org/details/img-dec-94
    - https://github.com/benletchford/systemless/issues/6193
    license: Maintainer-accepted promotional demo
    rights_holder: Access Software and original contributors
    permission: Accepted under the existing project policy for complete unchanged original promotional
      demos preserved from period cover-disc distribution. This records project-policy acceptance,
      not an affirmative publisher or artwork licence grant. No applicable explicit redistribution
      restriction was identified in the reviewed notices. Preserve every original file, notice and
      demo limit.
    notes: 'Unchanged original preserved StuffIt archive, demo 1.0.1. Independent unar/lsar and production
      packing match all 23 files, 46 forks and 23 Finder records with strict EOF. Ten literal classic
      Mac slash characters are explicitly mapped between production F02F, lsar colon and unar underscore
      representations; original archive names and bytes are unchanged. The full December 1994 Inside
      Mac Games ISO matches its published size and SHA1. Its Demos/Links Pro folder contains exactly
      the same 23 relative files: every data fork, all 413 ordered resource payload/name/attribute
      records and type/creator match. One raw resource container and two Finder flags differ; whole-archive
      or raw-fork identity is not claimed. October 1994 contains 18 additional saved-shot/launcher
      files and is not whole-package proof. The full 1366-byte demo instructions, 1894-byte speech
      Read-Me and 54 standard UI/string/version records were reviewed with no parse errors. No opaque
      game/course/animation/recorded-shot or speech-macro logic was decoded. No files were stripped,
      reconstructed, replaced or changed; preserve all sounds, animations, speech macro document,
      recorded games, notices and single-hole demo limit.'
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/links-pro-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6193
    rights_holder: Access Software and original contributors
    permission: Real content-only capture of the project-policy-accepted promotional demo.
    notes: Unscaled, unretouched 640x400 lossless crop of actual native gameplay during follow-through
      after the documented swing input. Excludes desktop and menu bar. No separate artwork licence
      grant claimed.
references: *id001
---

Play one Harbour Town hole in the original Macintosh **Links Pro** demo.

Choose **File → New Game**, select a golfer, click **Add**, then confirm the setup.
To swing, hold the **SWING** button for the backswing, release near the top of the
meter, then click again as the indicator returns to the bottom. The bundled
instructions explain club selection and shot timing.

A native shot and normal penalty/drop recovery are verified. Browser qualification
remains pending.

![The golfer following through after a shot](incoming/links-pro-demo/gameplay.png)
