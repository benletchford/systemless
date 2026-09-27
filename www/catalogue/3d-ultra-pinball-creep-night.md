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
      BasiliskII; score and ball position differ, so pixel parity is not claimed. Browser launch
      is unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3044
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
run also launched and scored a ball, though its score differed. Browser
launch remains disabled until manual testing and approval.
