---
id: power-pong-10
kind: game
title: PowerPong 1.0
summary: Rally a speeding ball past the opposing paddle in a fast arcade match.
developer: Caveman Creations
publisher: Caveman Creations
year: 1995
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
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      The unchanged shareware package reached an active 68K match with its
      companion music, sound, and sprite files. B began the game. At the same
      guest tick, Shift moved the player-one paddle up, Option moved it down,
      and the idle run left it centered. The release browser build also started
      an active match with B and showed the left paddle moving to the top with
      Shift and to the bottom with Option.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3823
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/power-pong-10.hqx
    expected_sha256: 2ba49f7bd30ac454e0a1bf1b3aabc6103a82ccde40bbd5ed152859aad1275aa8
    expected_size: 2178479
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/power-pong-10.hqx
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    rights_holder: Caveman Creations
    permission: >-
      David Hay's author-submitted Info-Mac abstract explicitly permits
      PowerPong in CD-ROM collections. The bundled Readme identifies it as
      shareware. This entry preserves the complete original package for free
      online play.
    notes: >-
      Original 2,178,479-byte BinHex/StuffIt archive, SHA-256
      2ba49f7bd30ac454e0a1bf1b3aabc6103a82ccde40bbd5ed152859aad1275aa8.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/power-pong-10/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3823
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only active match crop; emulator framing is excluded.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/power-pong-10.hqx
---

## Return the ball

![PowerPong active match](incoming/power-pong-10/gameplay.png)

Press **B** on the menu to begin. Use **Shift** to move the left paddle up and
**Option** to move it down; **Space** makes it move faster. The ball speeds up
when it hits the end of a paddle. The first player to 15 points wins.
