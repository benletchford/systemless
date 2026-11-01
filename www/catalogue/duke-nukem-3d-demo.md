---
id: duke-nukem-3d-demo
kind: game
title: Duke Nukem 3D Demo
summary: >-
  Blast aliens across Los Angeles in the Power Macintosh demo of 3D Realms'
  irreverent first-person shooter.
developer: 3D Realms; Lion Entertainment (Macintosh)
publisher: MacSoft
year: 1996
architectures:
- ppc
default_architecture: ppc
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: dc0b47fb deterministic play runner
    architecture: ppc
    environment: "The unchanged demo archive ran natively on the PowerPC slice at depth 8 with the C-string glue, PPostEvent, Open Transport errno and writable self-modifying code support from #5475. The 3D Realms logo and title led through New Game, episode and skill to the Hollywood Holocaust rooftop with the Build engine view and status bar. Holding Up for 90 ticks walked Duke forward. A matched control without the key was identical until the input and then differed by 156,493 pixels. An exact replay matched both checkpoints, with zero exhausted frames. Combat, later levels, audio and browser performance remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5476
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 81da99bd browser build with launch flag and 60 MHz adaptive pacing
    architecture: ppc
    environment: "Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs of a locally served trunk build with #5475 used the exact pinned original archive. Scripted ordinary input passed the logo and title and held Up; the attract-mode level ran behind the main menu. At the default 25 MHz budget two of three samples had a single 51-52 ms frame; with runtime pacing at 60 MHz (adaptive minimum 25 MHz) samples reached 62.24, 62.24 and 63.34 guest ticks per second at 60.2 host fps with no console errors. This approves bounded browser launch; browser gameplay beyond the menu, audio and later levels remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5476
runtime:
  executable_path: Duke Nukem 3D Demo PPC/Duke Nukem 3D Demo
  runtime_pacing:
    adaptive_min_cpu_mhz: 25
    cpu_mhz: 60
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 670b7bef4c660b5d1fce4ab40437dc0720afa20a0149e6d15f523725d513341c
    size_bytes: 5521169
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/duke-nukem-3d
    - https://download.classicmacdemos.com/Duke%20Nukem%203D%20Demo%20PPC.sit
    - https://github.com/benletchford/systemless/issues/5476
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: 3D Realms / Gearbox Software; MacSoft
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on at least 36 period Macintosh magazine cover discs, more than any other
      demo not yet in the catalogue. The bundled documents contain no explicit
      redistribution grant or restriction. Preserve the complete unchanged archive and remove it
      if a rights holder objects.
    notes: PowerPC demo with the shareware L.A. Meltdown levels. Requires 256 colours.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 89fc828fcd23a5a85c3db55ce5613c273adb5b0955c0e164ef895b6d573b4d4e
    size_bytes: 61892
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5476
    permission: >-
      Fresh gameplay capture from the unchanged demo archive. The underlying game
      artwork remains its owners' property.
    notes: >-
      Native PowerPC Hollywood Holocaust rooftop after walking forward, cropped to
      the game view.
references:
- https://classicmacdemos.com/duke-nukem-3d
- https://github.com/benletchford/systemless/issues/5476
---

## Come get some

![Duke Nukem 3D Demo](https://assets.systemless.org/catalogue/media/sha256/89/89fc828fcd23a5a85c3db55ce5613c273adb5b0955c0e164ef895b6d573b4d4e.png)

Fight through the opening Los Angeles levels of Duke Nukem 3D, with jetpacks, pipe bombs and plenty of attitude.

Press a key on the title screen, then Return to choose **New Game**, an episode and a skill level. Use the arrow keys to move and turn, Control to fire, the A key to jump and the space bar to open doors.
