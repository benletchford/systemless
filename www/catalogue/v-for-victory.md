---
id: v-for-victory
kind: game
title: V for Victory
summary: Command the Utah Beach campaign in Atomic Games' original one-scenario Macintosh demo.
developer: Atomic Games
publisher: Three-Sixty Pacific
year: 1992
architectures: [68k]
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: 0.58.0 + clean deterministic runner
    architecture: 68k
    environment: >-
      Loaded the unchanged StuffIt demo with its non-FPU executable in an
      800-by-600, 256-colour display. Dismissed the FPU advisory, watched the
      original logos, selected Begin New Game, reached the Utah Beach map and
      dismissed its opening staff briefing. Release-mode browser verification
      remains pending.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2684
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/v-for-victory/
    download_page: https://classicmacdemos.com/v-for-victory
    expected_sha256: 16bfcadb6fb02c5c2d6ee8375cda6579571f5743c05215645fafc44b4a9ca09c
    expected_size: 982399
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/v-for-victory
    - https://static.classicmacdemos.com/demos/v-for-victory/README.txt
    - https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
    rights_holder: Atomic Games, Three-Sixty Pacific and successors
    permission: >-
      This is the original promotional Macintosh demo, not a retail battleset.
      Its bundled read-me says the demonstration copy includes only the first
      scenario, and its opening card says Save and Restore are disabled.
      Apple's 1992 Macintosh Demo Games CD independently distributed a variant
      of the same limited demo. No express redistribution licence was found in
      the standalone package.
    notes: >-
      Unchanged 982,399-byte StuffIt 5 archive, SHA-256
      16bfcadb6fb02c5c2d6ee8375cda6579571f5743c05215645fafc44b4a9ca09c.
      It contains the original FPU and non-FPU 68K applications, one Utah Beach
      scenario and supporting data, plus the original read-me.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/v-for-victory/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2684
    permission: >-
      Fresh gameplay capture made for this catalogue. The underlying artwork
      remains the property of its rights holders.
    notes: >-
      Cropped the 792-by-510 game map and controls at (5,24) from an
      800-by-600 Systemless framebuffer after dismissing the opening staff
      briefing. The crop excludes the Macintosh menu bar and desktop without
      altering game pixels.
references:
- https://classicmacdemos.com/v-for-victory
- https://static.classicmacdemos.com/demos/v-for-victory/README.txt
- https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
---

## One campaign on Utah Beach

![Utah Beach map in the V for Victory Macintosh demo](incoming/v-for-victory/gameplay.png)

V for Victory puts you in command of Allied or Axis units on the Cotentin
Peninsula. Select a side and plan movement, attacks and support across a
hexagonal map while the staff assistant reports the changing situation.

This is Atomic Games' original Macintosh demonstration, not the complete
commercial series. It includes only the introductory Utah Beach scenario;
Save and Restore are disabled. At startup, the non-FPU application may advise
using the FPU version on a suitable Macintosh. Press Return to continue.
