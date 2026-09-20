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

Systemless reaches the shareware screen, main menu and Play mode from the exact
archive. A deterministic run entered the live field with the player's ship and
then verified a changed field after Up and Space input, demonstrating gameplay
beyond launch.
