---
id: powermonger-demo
kind: game
launch_enabled: true
title: PowerMonger Demo
summary: >-
  Explore a living 3D landscape and begin a conquest in the original Macintosh
  strategy demo.
developer: ATREID Concept
publisher: Electronic Arts
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: dcf21abe native
    architecture: 68k
    environment: >-
      Complete unchanged period MacBinary at original/default machine settings. The
      declared Command-N menu shortcut starts a new conquest. Hover over the first
      territory, then click to enter live 3D play. Clicking the overview map changes the
      landscape view. Matched input/omission runs have identical pre-input RGB frames; the
      active landscape changes 57059 pixels versus 127 in the idle control, with 57186
      differing pixels between final views. Both runs finish 824 frontend and 1424
      guest ticks with no exhausted frames. Actual captures and external image comparisons
      were inspected; scripts have zero assertions. No game files, preferences, sound,
      CPU, partition or display settings were changed. This verifies bounded world
      entry and overview-camera input, not conquest completion, saves, audio fidelity,
      independent hardware accuracy or browser pacing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6172
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 843a23d2 private release preview
    architecture: 68k
    environment: >-
      Fresh private HTTPS browser active and omitted-input runs using the exact
      hosted original archive at default 25 MHz, display, partition and sound.
      Each case passes a 20-second new-conquest/world-initialization and camera
      window plus a separate 20-second sustained world window. Original Command-N
      and territory hover/click reach live play. Overview input changes 57132
      landscape pixels versus 149 in the control; pre-input RGB frames match
      exactly. Actual input and sustained captures were inspected. Endpoint guest
      rates are 59.09-60.00 ticks/sec and host rates 60.00-62.26 FPS; maximum runtime
      frames are 16.2-16.4 ms and minimum audio queue is 115.9 ms. No action,
      lifecycle or network errors. Original archive and main/worker Wasm response
      bodies match pins; the executing worker module is independently pinned and
      its Debugger disabled afterward. All observed targets resume. No game files,
      preferences or machine settings change. This is bounded private browser
      world/camera approval, not released public verification, completion, saves,
      audio fidelity or independent hardware accuracy. Runtime source matches the
      merged catalogue source except the private launch flag; draft runtime
      optimizations are excluded.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6172
runtime:
  executable_path: Demo
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 6941f83b1228c0a44aa297fea0426a217821cf6fa6405524270559963bea4877
    size_bytes: 1707008
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://discmaster.textfiles.com/browse/26992/macformat-020.iso/Demos/MF20/PowerMonger/Demo
    - https://archive.org/details/macformat-020
    - https://classicmacdemos.com/powermonger
    - https://github.com/benletchford/systemless/issues/6172
    license: Maintainer-accepted promotional demo
    rights_holder: Electronic Arts, ATREID Concept and original contributors
    permission: >-
      Accepted under the existing project policy for complete unchanged original
      promotional demos preserved from period cover-disc distribution. This records
      project-policy acceptance, not an affirmative publisher or third-party licence grant. No
      applicable explicit redistribution restriction was identified in the reviewed
      notices. Preserve original files, credits, notices and demo limits.
    notes: >-
      Canonical complete application from MacFormat Disc 20, January 1995. Production
      packing matches its one file, two forks and Finder record with strict EOF. All
      144 ordered resource type/ID/name/attribute/payload records match the established
      web demo, including repeated IDs; data forks are empty. Resource map wrappers and
      application filenames differ, so whole-file identity is not claimed. All 23
      standard string/version records and five dialog-item records were parsed and read.
      The actual About picture credits ATREID Concept and Electronic Arts. Three TEXT
      resources named TEXTURES are binary assets and were excluded from notice-text
      review. No standalone Readme is included. No guest instruction, opaque game asset or
      script was reverse engineered.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 98cbc8aa2af268910b9817abdd15cdd26cda3a089e9668363f7f4f99d0e52d13
    size_bytes: 216717
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6172
    rights_holder: Electronic Arts, ATREID Concept and original contributors
    permission: Real content-only capture of the project-policy-accepted original demo.
    notes: >-
      Unscaled 512x363 crop of actual native play after an overview-map click.
      Excludes desktop, title frame and Classic menu bar. Pixels are unretouched; no separate
      artwork licence is claimed.
references:
- https://classicmacdemos.com/powermonger
- https://archive.org/details/macformat-020
- https://github.com/benletchford/systemless/issues/6172
---

Begin a conquest in **PowerMonger Demo 1.2**, the original Macintosh strategy demo.

At the title, use **Command-N** for **Conquest → Start New Conquest**. Hover over
the **first territory in the upper-left corner**, then click to enter the world.
Click the small **overview map** on the left to change the landscape view.
World entry and overview-camera input are verified in a private browser preview.

![Exploring the first territory in PowerMonger](https://assets.systemless.org/catalogue/media/sha256/98/98cbc8aa2af268910b9817abdd15cdd26cda3a089e9668363f7f4f99d0e52d13.png)
