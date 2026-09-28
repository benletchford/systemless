---
id: 3d-ultra-pinball-creep-night
kind: game
title: "3-D Ultra Pinball: Creep Night Demo"
summary: >-
  Send a ball through Creep Night's haunted pinball table in Sierra's original
  Macintosh demo.
developer: Dynamix
publisher: Sierra On-Line
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 38865111bdb6477b5a4b0d6a6726cf9d3445b8e8
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged demo reached the Creep Night
      table, actuated a flipper with Shift, then launched a ball with Down Arrow and
      advanced the score to 205,000. The same script launched and scored a ball in
      BasiliskII, which reached 115,000 at that checkpoint; score and ball position
      differ, so pixel parity is not claimed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3044
  - date: "2026-09-29"
    tester: Catalogue maintainer
    systemless_version: b546fda4d2f97be6e4bd32c50b97aa59915d59a7
    architecture: 68k
    environment: >-
      Chrome 151 on the local release-mode Pages build loaded the unchanged
      archive through its public asset URL. Clicking the demo's start prompt
      reached Player 1 Ready; holding and releasing Down Arrow launched a ball
      and advanced the score from 0 to 202,000. Holding Shift raised the right
      flipper. The 55-second run had no console errors, approximately 60 host
      frames and 61 guest ticks per second, and a 24 ms maximum runtime frame.
      This verifies browser input and gameplay, not pixel parity.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3345
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 6d80c8d722036de2bca31557fb8bf60e9076b4f95b3b544c07e5f9fda2a4e90d
    size_bytes: 4013943
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/3d-ultra-pinball-creep-night
    - https://download.classicmacdemos.com/Creep%20Night%20Demo.sit
    rights_holder: 3-D Ultra Pinball rights holders
    permission: >-
      This is the unchanged purpose-built Macintosh promotional demo, not the retail
      release. Classic Macintosh Game Demos documents copies on 16 contemporary cover
      discs. The archive has no express redistribution clause; no retail table or
      modified application is substituted.
    notes: >-
      Original 4,013,943-byte StuffIt archive, SHA-256
      6d80c8d722036de2bca31557fb8bf60e9076b4f95b3b544c07e5f9fda2a4e90d. The tested Creep Night Demo application has a
      runnable 68K CODE resource.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2ff3fc96f282a96cc3d5493417803f204b6e459a055c5a283d4a6d9a56ff8cda
    size_bytes: 651858
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3044
    permission: >-
      Fresh gameplay capture from the unchanged promotional demo for this catalogue
      entry. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 640-by-480 game-content crop at (80,80) of a deterministic 800-by-600
      Systemless framebuffer after a scored ball launch. The crop excludes the Classic Mac
      menu bar, desktop, and window framing without changing game pixels. PNG SHA-256
      2ff3fc96f282a96cc3d5493417803f204b6e459a055c5a283d4a6d9a56ff8cda, 651,858 bytes.
references:
- https://classicmacdemos.com/3d-ultra-pinball-creep-night
- https://download.classicmacdemos.com/Creep%20Night%20Demo.sit
---

## A haunted table

![Creep Night demo pinball table after a scored launch](https://assets.systemless.org/catalogue/media/sha256/2f/2ff3fc96f282a96cc3d5493417803f204b6e459a055c5a283d4a6d9a56ff8cda.png)

Creep Night turns the pinball table into a haunted castle full of ramps,
targets, skulls, and animated details. The Macintosh demo opens onto the
table; a flipper responds to Shift, and a Down Arrow plunger input starts a
ball that can score points. This capture shows the table after the score
advanced to 205,000.

This is Sierra's original promotional demo, not the retail game. Systemless
reached interactive play with the unchanged archive. A separate BasiliskII
run also launched and scored a ball, reaching 115,000 at that checkpoint.
Browser play also reached the table using the same archive: click the demo's
start prompt, hold and release Down Arrow to launch, and use Shift for the
flipper. The browser run launched and scored a ball without console errors.
