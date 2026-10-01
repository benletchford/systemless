---
id: farm-patrol
kind: game
title: F.A.R.M. Patrol 1.0
summary: Drive an armored patrol vehicle through a mutant-filled wasteland.
developer: Five Guys from Stanford
publisher: Five Guys from Stanford
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + deterministic runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged F.A.R.M. Patrol 1.0 archive launched in 68K Systemless.
      File > New Game opened the Level 1 vehicle and terrain scene. The
      Controls dialog identifies numeric keypad 6 as accelerate. In matched
      60-tick replays, holding that key moved the vehicle to the right while
      the idle run kept it near the starting position. The release browser
      build fetched the same archive once, reached the active level, and
      visibly moved the vehicle right when numeric keypad 6 was held.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3799
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/farm-patrol-10.hqx
    expected_sha256: da9b7bec1b89f2467f90172290a73a5b2d1fcda50a2b3333c45e3c8ffa211b4d
    expected_size: 1311567
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/farm-patrol-10.hqx
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    rights_holder: Five Guys from Stanford
    permission: >-
      The author-submitted Info-Mac abstract calls the game freeware and
      permits copies when the original documentation is included. This entry
      preserves the complete, unchanged package with its README and sample
      saved games for free online access.
    notes: >-
      Original 1,311,567-byte BinHex/StuffIt package, SHA-256
      da9b7bec1b89f2467f90172290a73a5b2d1fcda50a2b3333c45e3c8ffa211b4d.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/farm-patrol/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3799
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware package.
      Underlying artwork remains its owners' property.
    notes: >-
      Full 800-by-600 Systemless framebuffer after accelerating through
      Level 1 with numeric keypad 6.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/farm-patrol-10.hqx
---

## Shoot before you jump

![F.A.R.M. Patrol on Level 1](incoming/farm-patrol/gameplay.png)

Choose **File > New Game** to start. The default controls are numeric keypad
**6** to accelerate, **4** to slow down, **0** to jump, and **Space** to fire.
You can change them under **Options > Controls**. Cross eight levels of
craters and mutant creatures while managing your ammunition and time.

The authors released F.A.R.M. Patrol as freeware. The complete original
package includes the README and two sample saved games.
