---
id: star-wars-rebel-assault-ii-demo
kind: game
title: "Star Wars: Rebel Assault II — The Hidden Empire Demo"
summary: Fly and fire through The Mining Facility in LucasArts' original one-chapter demo.
developer: LucasArts
publisher: LucasArts
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 97bb8282 (c5f7760f native-equivalent runtime)
    architecture: 68k
    environment: >-
      Complete unchanged original SIT, original machine defaults, and the genuine
      shipped 68k executable. Normal Start Game / Add New Pilot / Standard setup
      reaches the provided Chapter 6, The Mining Facility. Its cockpit gameplay
      renders damage, pilot and score displays. A matched fire/omitted-button pair
      starts from RGB-identical frames after the same aiming move. Holding the
      mouse button for twelve ticks produces inspected red laser bursts absent
      from the control: 226040, 384 and 3252 pixels differ at held ticks 4, 8 and
      12. The frames match again sixty ticks after release. Both runs finish 4732
      frontend and 5332 guest ticks with zero exhausted frames. Each passes two
      menu-readiness assertions; the firing evidence is the inspected captures
      and external RGB comparison, not those setup assertions. This establishes
      bounded firing, not a complete chapter, audio or browser qualification.
      Frozen binary b3d7252a uses source 97bb8282, matching c5f7760f runtime source
      and preceding the later 6363b91e presentation changes.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6090
runtime:
  executable_path: Rebel Assault II Demo/Rebel Assault II Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/star-wars-rebel-assault-ii-the-hidden-empire/
    download_page: https://classicmacdemos.com/star-wars-rebel-assault-ii-the-hidden-empire
    expected_sha256: b08c5741a0e7932a023c5107a15a9125f9e7b220872fd40c7d53477c58ff86a6
    expected_size: 79242123
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/star-wars-rebel-assault-ii-the-hidden-empire
    - https://archive.org/details/IMG35March1996
    - https://github.com/benletchford/systemless/issues/6090
    license: Maintainer-accepted promotional demo
    rights_holder: LucasArts Entertainment Co. and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed notices. Preserve
      the complete original archive, all sixteen files, joystick utilities/settings,
      credits, notices and original demo limits. Only the provided chapter is tested;
      no passcode or chapter unlock is used.
    notes: >-
      All sixteen demo-folder files, thirty-two raw forks, all 524 resource payloads,
      names, IDs and attributes, and all sixteen Finder type/creator/flag records
      exactly match Inside Mac Games disc 35, March 1996. No period-wrapper or
      whole-disc byte identity is claimed. Full production packing without path
      filtering also matches independent unar extraction on all sixteen files,
      thirty-two forks and sixteen Finder records. The complete 14012-byte main
      Read Me and 4493-byte joystick Read Me were reviewed. The main Read Me dates
      demo version 1.0 to 4 December 1995. Notice screening covered 165 included
      standard UI/text/version resources, without standard compression. Opaque
      game Data and guest code/scripts were not decoded; no artwork-wide text
      extraction or independent hardware accuracy is claimed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/star-wars-rebel-assault-ii-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6090
    rights_holder: LucasArts Entertainment Co. and original contributors
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 640x400 crop of actual native Mining Facility cockpit gameplay,
      with the damage, pilot and score displays visible. Excludes desktop, Classic
      Mac menu bar and outer letterbox framing. This capture itself does not claim
      firing; the separate matched pair verifies that input. Original artwork
      retains its copyright.
references:
- https://classicmacdemos.com/star-wars-rebel-assault-ii-the-hidden-empire
- https://coverdiscs.com/disc/inside-mac-games-vol-4-issue-3/
- https://archive.org/details/IMG35March1996
- https://github.com/benletchford/systemless/issues/6090
---

![In the cockpit in The Mining Facility](incoming/star-wars-rebel-assault-ii-demo/gameplay.png)

From Game Main Menu, choose Start Game, add a Standard pilot, then select
Chapter 6 — The Mining Facility. Move the mouse to aim and hold its button to fire.
The complete original demo retains its readmes, joystick utilities and settings.

Native testing verifies bounded laser firing against an omitted-button control.
A complete chapter and browser compatibility remain unverified; browser launch
stays disabled pending qualification.
