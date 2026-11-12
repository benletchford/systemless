---
id: virtual-pool-demo
kind: game
title: Virtual Pool 4-Ball Demo
summary: Break the rack in the original Macintosh Virtual Pool preview.
developer: Celeris
year: 1996
architectures:
- ppc
default_architecture: ppc
category: Simulation
compatibility:
  status: unknown
  verified:
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: ppc
    environment: >-
      Original unchanged demo at default machine, preferred partition, display, sound
      and preferences. Click the title and dismiss shooting help with Return. Press
      and release S to enter Shoot Mode, then hold S while moving the mouse backward
      and forward to stroke the cue. Active and omission runs begin with identical
      RGB captures and both complete 1230 frontend/guest ticks with zero exhausted
      frames and zero script assertions. Actual images show the active backswing and
      broken rack with moving balls; omission remains racked. Bounded native shot only,
      not complete-game, network, saves, audio-fidelity, hardware or browser-pacing
      qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6190
runtime:
  executable_path: Virtual Pool Demo ƒ/Virtual Pool 4-Ball Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Virtual%20Pool%20Demo.sit
    expected_sha256: ba83596ae98561f52831826f3981e316f4e86e72350b25246bda0d8401142c63
    expected_size: 980924
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/virtual-pool
    - https://archive.org/details/Mac_Addict_-_The_Disc_February_1997
    - https://coverdiscs.com/disc/macaddict-issue-006
    - https://github.com/benletchford/systemless/issues/6190
    license: Maintainer-accepted promotional demo
    rights_holder: Celeris, Interplay Productions and original contributors
    permission: >-
      Accepted under the existing project policy for complete unchanged original
      promotional demos preserved from period cover-disc distribution. This records
      project-policy acceptance, not an affirmative publisher or artwork licence grant.
      No applicable explicit redistribution restriction was identified in the reviewed
      notices. Preserve every original file, notice and demo limit.
    notes: >-
      Unchanged preserved StuffIt archive, version 1.8.6. Independent extraction and
      production packing match all 14 files, 28 forks and 14 Finder records with strict
      EOF. The original February 1997 MacAddict 006 installer yields the same files,
      forks and Finder records beneath one extra outer folder, including NetSprocketLib
      and Register with MacPlay. The full period ISO matches its published size and
      SHA1. This is file-payload parity, not whole-installer or archive-wrapper identity.
      The full 15227-byte README matches the preserved public README. All 107 selected
      standard application/component/registration notice, UI and version records and
      29 readable installer UI/string/version records were reviewed. Disclosed review
      limit: 27 compressed installer UI/string records remain unread; no exhaustive
      installer-path or complete rights-grant review is claimed. The November 1996
      disc contains a different 0.9 demo and is not exact-package proof. No game files
      were reconstructed, removed or replaced; opaque game data was not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/virtual-pool-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6190
    rights_holder: Celeris, Interplay Productions and original contributors
    permission: Real content-only capture of the project-policy-accepted promotional demo.
    notes: >-
      Unscaled, unretouched 600x450 lossless crop of the actual native table after a
      documented cue stroke breaks the rack. Excludes menu bar and desktop. No separate
      artwork licence grant claimed.
references:
- https://classicmacdemos.com/virtual-pool
- https://archive.org/details/Mac_Addict_-_The_Disc_February_1997
- https://github.com/benletchford/systemless/issues/6190
---

Play the original Macintosh **Virtual Pool** four-ball preview. Hit the lowest
numbered ball first and try to pocket the 9 ball.

Click the title and press **Return** to dismiss the shooting help. Press and release
**S** to enter Shoot Mode, then hold **S** while moving the mouse backward and forward
to stroke the cue. Press **Escape** for menus and the built-in control help.

A native break shot is verified. Browser qualification remains pending.

![The balls spreading across the table after a break shot](incoming/virtual-pool-demo/gameplay.png)
