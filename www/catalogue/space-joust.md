---
id: space-joust
kind: game
title: Space Joust 2.0
summary: >-
  Pilot a lone ship through Mind-ware's shareware space-combat arena.
developer: John M. Dole
publisher: Mind-ware
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: Deterministic gameplay run from the complete unchanged Space Joust 2.0 archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2312
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: c3ff69bde46cd5147939565d4143341669b8bba6db845040711ce92c0c50070e
    size_bytes: 852613
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/s/
    - https://www.vintageapplemac.com/files/games/Space%20Joust%202.0.sit
    license: Space Joust Manual distribution terms
    rights_holder: John M. Dole / Mind-ware
    permission: >-
      The bundled Space Joust Manual states: “Unless explicitly stated in writing,
      Mind-ware does not grant permission to distribute this software for profit
      in any form, including but not limited to, electronic information service
      distribution, bulletin board distribution, and magnetic or optical medium
      distribution. Non-profit distribution of the software is acceptable without
      prior written notice, providing that the software is not modified in any way,
      and the complete works of the software are included in the distribution
      package.”
    notes: >-
      Unchanged 852,613-byte StuffIt archive, pinned by SHA-256
      c3ff69bde46cd5147939565d4143341669b8bba6db845040711ce92c0c50070e. The
      complete package retains the 68k application, manual, story, ship files,
      backgrounds, sounds, sprites, data, icon and blueprint.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: e116763d22be409a72a8c606e5c5a025a9943b0cdd7c28825b59e0187f97d6cc
    size_bytes: 8253
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2312
    permission: >-
      Original screenshot captured from the exact Space Joust archive during a
      deterministic gameplay run for this catalogue. Underlying game artwork
      remains the property of its rights holders.
    notes: >-
      Fresh capture after entering Play mode and processing Up and Space input,
      cropped to the 640-by-480 game content surface. It excludes the host
      desktop, emulator margins and Classic Mac menu bar.
references:
- https://www.vintageapplemac.com/software/games/s/
- https://www.vintageapplemac.com/files/games/Space%20Joust%202.0.sit
---

## Preserved as the complete non-profit archive

Space Joust 2.0 is a Mind-ware space-combat game for 68k Macs. This entry
references the unchanged StuffIt archive and keeps its application, supporting
files, manual and distribution terms together.

The bundled Space Joust Manual allows non-profit distribution when the software
is not modified and the complete works are included. It does not grant
for-profit distribution, so this catalogue record is limited to the unchanged
complete package for non-profit hosting and redistribution.

## In the live arena

![Space Joust gameplay](https://assets.systemless.org/catalogue/media/sha256/e1/e116763d22be409a72a8c606e5c5a025a9943b0cdd7c28825b59e0187f97d6cc.png)

Systemless reaches the shareware screen, main menu and Play mode from the exact
archive. A deterministic run entered the live field with the player's ship and
then verified a changed field after Up and Space input, demonstrating gameplay
beyond launch.
