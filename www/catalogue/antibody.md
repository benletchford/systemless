---
id: antibody
kind: game
title: Antibody 1.0.0
summary: >-
  Shrink to a single cell and clear a hostile bloodstream in Slimyfrog
  Software's colourful action shooter.
developer: Marco Carra
publisher: Slimyfrog Software
year: 1997
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
    environment: >-
      Deterministic headless gameplay run from the complete original
      Antibody 1.0.0 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2308
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: b08c2bdc602cd5e6888776376e52ef9e41a2875985a7de8a20222a573a4557b4
    size_bytes: 2145256
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/a
    - https://www.vintageapplemac.com/files/games/Antibody%201.0.0.sit
    license: Slimyfrog Software License (bundled Software License, section 2)
    rights_holder: Marco Carra and Slimyfrog Software
    permission: >-
      The bundled Software License section 2 states that non-profit distribution
      is permissible only when the software is not altered in any way and is
      distributed in its entirety. It requires written consent for electronic
      transfer, renting, leasing, loaning, selling or other distribution when
      done for profit, and prohibits modification or alteration including
      decompiling, disassembling, reverse engineering, or creation of works
      arising from the software. This entry therefore retains the complete,
      unchanged archive and is limited to non-profit distribution; use remains
      subject to the section 1 trial and registration terms.
    notes: >-
      Unchanged 2,145,256-byte StuffIt archive, SHA-256
      b08c2bdc602cd5e6888776376e52ef9e41a2875985a7de8a20222a573a4557b4.
      The complete archive contains the fat Antibody application, Backgrounds,
      Pictures, Scores, Sounds and Sprites data, About Antibody, How to Register,
      Software License, Installer Log File and Icon. The verified Systemless path
      is the archive's 68k application slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 6a170ee1dbd35fe21eabf469723e4acd994780df3e9d74c94c4acd4cd175d423
    size_bytes: 232572
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2308
    permission: >-
      Original screenshot captured from the exact Antibody archive during a
      deterministic gameplay run for this catalogue. Underlying game artwork
      remains the property of its rights holders.
    notes: >-
      Fresh capture after Level 1 movement and shooting input, cropped to the
      650-by-490 game window. It excludes the host desktop, emulator margins and
      Classic Mac menu bar.
references:
- https://www.vintageapplemac.com/software/games/a
---

## A mission inside the bloodstream

![Antibody gameplay](https://assets.systemless.org/catalogue/media/sha256/6a/6a170ee1dbd35fe21eabf469723e4acd994780df3e9d74c94c4acd4cd175d423.png)

Antibody puts the player inside a sick research subject, shrinking the ES-21 to
the size of a cell and sending it through a living bloodstream. Clear the viral
invaders quickly, avoid red blood cells, collect bonus tokens and choose a new
scenario as the levels advance. The colourful presentation combines scrolling
action with a light science-fiction mystery about the sabotage behind the
infection.

## Preserved as the complete non-profit archive

The bundled Software License permits non-profit distribution only when the
software is unaltered and distributed in its entirety. It also requires written
consent for for-profit distribution and prohibits modification, including
decompilation, disassembly, reverse engineering and derivative works. The
catalogue therefore references the unchanged StuffIt archive with its
application, data files, documentation and licence intact.

Systemless was tested beyond startup from the exact archive using its verified
68k path. After the registration dialog was dismissed and keyboard-plus-mouse
control enabled, the run selected Level 1, entered the live bloodstream and
processed movement and shooting input. The HUD changed from `Bombs: 1450` to
`Bombs: 1250` and then `Bombs: 1000`, with moving sprites visible before and
after the actions.
