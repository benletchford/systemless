---
id: avernum-2-demo
kind: game
title: Avernum 2 Demo
summary: >-
  Lead a party through the caves of Avernum as its people defend their home from
  the Empire in Spiderweb Software's original fantasy RPG sequel.
developer: Jeff Vogel
publisher: Spiderweb Software
year: 2000
architectures:
- ppc
default_architecture: ppc
launch_enabled: false
category: Role-Playing
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: de69e8d9 native candidate
    architecture: ppc
    status: playable
    environment: >-
      Complete original v1.0.2 Macintosh demo. Initial runtime settings and default
      party preserved. Choose the game's Switch Resolution This Time option and
      continue through the opening story to Fort Ganrick. Dismiss Instant Help.
      A terrain click moves the party and scrolls the scene. The matched no-input
      replay holds the original position. Both replays reach 4,950 frontend and
      guest ticks with zero exhausted frames. Browser pacing and input are pending.
    evidence: https://github.com/benletchford/systemless/issues/6015
runtime:
  executable_path: Avernum 2 v1.0.2 Demo Installer/Avernum 2 ƒ v1.0.2/Avernum 2 v1.0.2
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: url
    url: https://www.spiderwebsoftware.com/ftp/mac/Avernum2.demo.v102.bin
    expected_sha256: 25144b9247a53dde8aa2f37a37bb3c3791e4d77d27c974ef893309c2a39d9470
    expected_size: 9606912
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.spiderwebsoftware.com/avernum/avernum2old/index.html
    - https://www.spiderwebsoftware.com/ftp/mac/Avernum2.demo.v102.bin
    - https://github.com/benletchford/systemless/issues/6015
    license: Complete bundled Spiderweb Software License
    rights_holder: Spiderweb Software, Inc. and its licensors
    permission: >-
      Section 1 covers the accompanying software and documentation. Section 2
      permits nonprofit distribution of the complete unmodified software.
      The original unregistered state and 30-day use condition apply.
    notes: >-
      Complete original 9,606,912-byte MacBinary installer. The loader expands
      thirteen records, including the application, game data, manual and order
      form, map, support, licence and bundled DrawSprocketLib. Independent unar
      extraction verifies the outer installer forks. Expanded payload parity
      has not been independently verified.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/avernum-2-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6015
    permission: >-
      Original Systemless gameplay capture. Underlying artwork remains the
      property of Spiderweb Software and its licensors.
    notes: >-
      Fort Ganrick after a terrain click in the exact original demo. Lossless
      crop of the surrounding margin to 780 by 579 pixels.
references:
- https://www.spiderwebsoftware.com/avernum/avernum2old/index.html
- https://github.com/benletchford/systemless/issues/6015
---

![The default party exploring Fort Ganrick](incoming/avernum-2-demo/gameplay.png)

Begin at Fort Ganrick and guide a four-character party through subterranean
towns and caverns. Click the terrain or use the numeric keypad to move. This
is the original Classic Macintosh v1.0.2 demo with its manual, map and licence.
