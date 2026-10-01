---
id: please-shoot-me
kind: game
title: Please, Shoot Me! 1.0
summary: Shoot the fast moving creatures crossing a colorful arena.
developer: Slovis Software
publisher: Slovis Software
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged FAT shareware package launched in 68K Systemless with its
      companion Music files. After dismissing the shareware prompt and choosing
      Play, moving creatures crossed the arena. A click removed a visible
      creature in a matched run while the idle run retained it. The release
      browser fetched the original archive once, reached the same moving-target
      arena, and registered hits from browser clicks: the level result showed
      a score of 75 and five percent shot, compared with zero in an untouched
      browser run.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3818
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/shoot-me-10.hqx
    expected_sha256: 6cfabf85f02ebe81784c872a667a9d837f3735de75c9dfd8101e4e4e68a68f72
    expected_size: 1242579
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/shoot-me-10.hqx
    rights_holder: Slovis Software
    permission: >-
      The bundled Slovis Software Read Me permits free redistribution when the
      application, Read Me, Registration, and Music files remain together.
      It requires consent for distribution for profit or on CD. This entry
      preserves the complete original package for free online play.
    notes: >-
      Original 1,242,579-byte BinHex/StuffIt archive, SHA-256
      6cfabf85f02ebe81784c872a667a9d837f3735de75c9dfd8101e4e4e68a68f72.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/please-shoot-me/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3818
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owner's property.
    notes: >-
      Content-only crop of the release browser's active arena with moving
      targets; Classic Mac menu bar and emulator framing are excluded.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/shoot-me-10.hqx
---

## Hit the moving targets

![Please, Shoot Me! active arena](incoming/please-shoot-me/gameplay.png)

Choose **Not Yet** at the shareware prompt, then click **Play**. Aim and click
at the creatures crossing the arena. The original package includes its
required Music files.
