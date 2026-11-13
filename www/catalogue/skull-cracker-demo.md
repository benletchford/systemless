---
id: skull-cracker-demo
kind: game
title: Skull Cracker Demo
summary: >-
  Move through CyberFlix’s side-scrolling action game in the original Macintosh
  demo.
developer: CyberFlix
publisher: GTE Entertainment
year: 1996
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: 3d874b12 native
    architecture: 68k
    environment: "Complete unchanged nineteen-file original demo, default classic machine and preferred partition. Normal Begin reaches the actual level. Original Help shows W/A/S/D directions and J/K/P attacks. Fresh paired runs each finish 1354 frontend / 1954 guest ticks with zero exhausted frames and identical pre-input RGB. Only active holds default D/right for 30 ticks: the character walks right while omission remains left. Image differences are 51380 pixels after input and 64455 after continued simulation, including animated scene differences. Actual before/input/final captures inspected for both. Bounded level entry and movement only; combat, level completion, audio fidelity, hardware accuracy and browser pacing are unverified. No game, display, partition, difficulty or saved-state changes. Native proof does not approve browser launch."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6229
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: 3d874b12 native
    architecture: ppc
    environment: >-
      Same complete original archive and default machine. Actual startup and later
      captures both show Fatal error! ID = ^0. Sorry, returning to Finder. The
      placeholder appears literally; no specific error ID or cause is inferred. Requested actions
      finish 1202 frontend / 1202 guest ticks with zero exhausted frames, but this is
      not PPC gameplay. No unchanged retry.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/6230
runtime:
  executable_path: Skullcracker Demo/Skull
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 44fdc03c11f6db77ec4eed10667e0ef6697838407ed14233d539cdeadaf75333
    size_bytes: 6764866
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/skull-cracker
    - https://archive.org/details/IMG41Oct1996
    - https://github.com/benletchford/systemless/issues/6229
    license: Maintainer-accepted promotional demo
    rights_holder: CyberFlix, GTE Entertainment and original contributors
    permission: >-
      Accepted under the user-authorized existing project policy for complete
      unchanged period promotional demos. The original disc issuer explicitly permits
      distributing commercial game demos; this records project-policy acceptance, not an
      independently established publisher or artwork licence grant. No explicit restriction
      on intact demo sharing identified in reviewed notices. Retain all original files,
      notices and demo limits.
    notes: >-
      Complete unchanged received nineteen-file SIT, including every original movie,
      sound, bank and executable. All paths, 38 raw data/resource forks and Finder
      type/creator/flags exactly match Demos/Skullcracker Demo on Inside Mac Games Volume 4
      Issue 10 (October 1996). The entire 657811456-byte image matches published SHA-1
      e77132ca0725aa2072beb5abea76307946b97e9e. Independent real production packing
      preserves every path, fork and Finder record exactly with strict EOF. SIT wrapper
      is not claimed publisher-issued. Full 3665-byte period root Read Me and 12748-byte
      IMG FAQ reviewed; magazine material restrictions distinguished from the explicit
      commercial-demo distribution statement. Four standard DITL and four MENU
      resources parsed without errors and reviewed, plus the actual original Help screen. No
      guest instruction, game-script or opaque-asset decoding. The differing February
      1997 MacFormat variant was not used or mixed into this archive.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3818deb0f9f20e18a335232cd8a2a3c0cbf1eac5f69b918cda59f6c7f2cd78c9
    size_bytes: 120806
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6229
    rights_holder: CyberFlix, GTE Entertainment and original contributors
    permission: >-
      Actual content-only capture of the project-policy-accepted original demo; no
      separate artwork licence grant claimed.
    notes: >-
      Unscaled, unretouched 512x384 lossless pixel crop from the original native 68K
      level after D/right movement. Only the surrounding unused black framebuffer
      removed; complete game viewport and HUD retained.
references:
- https://classicmacdemos.com/skull-cracker
- https://archive.org/details/IMG41Oct1996
- https://github.com/benletchford/systemless/issues/6229
- https://www.mobygames.com/game/39769/skull-cracker/releases/
- https://github.com/benletchford/systemless/issues/6230
---

Choose **Begin** to enter the demo level. Move with **W/A/S/D**; use **J** to jump, **K** to kick, **P** to punch and **I** for inventory. The original **Help** screen shows the controls.

This is the complete original Macintosh promotional demo. The verified native path uses its 68K version; the PowerPC version currently stops at a startup error.
