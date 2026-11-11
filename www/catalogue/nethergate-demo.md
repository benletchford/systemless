---
id: nethergate-demo
kind: game
title: Nethergate Demo
summary: >-
  Lead Roman soldiers or Celtic rebels through two campaigns set in Roman
  Britain in Spiderweb Software's original fantasy RPG.
developer: Jeff Vogel
publisher: Spiderweb Software
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: de69e8d9 native candidate
    architecture: ppc
    status: broken
    environment: >-
      The complete original v1.0.1 demo enters the Roman campaign with the
      default party and display settings. Some artwork is clipped or missing
      during campaign selection and the introduction. A terrain click moves the
      party and scrolls the scene. The dismissed Instant Help window leaves
      visible remnants in the playfield. Browser play is pending.
    evidence: https://github.com/benletchford/systemless/issues/6020
runtime:
  executable_path: Nethergate Demo ƒ/Nethergate Demo v1.0.1
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mac/info-mac/_Game/nethergate-demo-101.hqx
    expected_sha256: aa5771048ac714f8dc183ae37ff4d195f047541972d479a68f12e9f1d3f92ea1
    expected_size: 7096724
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.spiderwebsoftware.com/nethergate/
    - https://ftp.funet.fi/pub/mac/info-mac/_Game/nethergate-demo-101.hqx
    - https://github.com/benletchford/systemless/issues/6020
    license: Complete bundled Spiderweb Software License
    rights_holder: Spiderweb Software, Inc. and its licensors
    permission: >-
      Section 1 covers the accompanying software and documentation. Section 2
      permits nonprofit distribution of the complete unmodified software.
      The original unregistered state and 30-day use condition apply.
    notes: >-
      Complete original Info-Mac publisher submission from March 1999.
      Independent extraction matched the loader's 12 files, 24 forks and
      12 Finder metadata records, including the manual, order form and licence.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/nethergate-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6020
    permission: >-
      Original Systemless gameplay capture. Underlying artwork remains the
      property of Spiderweb Software and its licensors.
    notes: >-
      Opening Roman campaign after a terrain click, with the menu bar removed.
      The capture shows the known Instant Help redraw defect.
references:
- https://www.spiderwebsoftware.com/nethergate/
- https://github.com/benletchford/systemless/issues/6020
---

![The default party at the start of the Roman campaign](incoming/nethergate-demo/gameplay.png)

Choose the Roman or Celtic campaign, then guide your party through Britannia
with the mouse or numeric keypad. This is the original Classic Macintosh
v1.0.1 demo with its bundled manual, order form and licence.
