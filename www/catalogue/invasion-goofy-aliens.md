---
id: invasion-goofy-aliens
kind: game
title: Invasion of the Goofy Aliens 1.0.1
summary: Aim a pea shooter at aliens popping out across a park before time runs out.
developer: Gooey Orbit Games
publisher: Gooey Orbit Games
year: 2001
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
    systemless_version: 0.73.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened its 68K application, displayed the title
      menu, accepted Begin New Game, showed the first level objective, and reached the
      active park scene with moving aliens, a timer, and mouse aiming and shooting. The
      release browser fetched the same archive and reached the live level.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3845
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 706345ba1bc46d8cf676256196445811ce7e0315e1fd002ebcf6862738ae31d5
    size_bytes: 2528418
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/invasion-aliens-101.hqx
    rights_holder: Gooey Orbit Games
    permission: >-
      The bundled Read Me permits free redistribution of the complete, unchanged
      archive. It also permits CD-ROM distribution under the same condition.
    notes: >-
      Original 2,528,418-byte Info-Mac BinHex/StuffIt package containing the
      application and Read Me; SHA-256
      706345ba1bc46d8cf676256196445811ce7e0315e1fd002ebcf6862738ae31d5.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4cd508a750704ae5720e7fa6033ea39c873bb711f202f4075b05c256c1b98236
    size_bytes: 268003
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3845
    permission: >-
      Fresh Systemless gameplay capture from the unchanged redistributable package.
      Underlying artwork remains its owner's property.
    notes: >-
      Exact 512-by-343 game-content crop at (144,129) from an 800-by-600 Systemless
      framebuffer during the first active park level; host and emulator framing excluded.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/invasion-aliens-101.hqx
---

## Chase the aliens away

![Aliens emerging in the park](https://assets.systemless.org/catalogue/media/sha256/4c/4cd508a750704ae5720e7fa6033ea39c873bb711f202f4075b05c256c1b98236.png)

Choose a difficulty and click **Begin New Game**. The first park level asks
you to hit 40 aliens in 60 seconds. Aim with the mouse and click to shoot
as aliens pop out from behind the scenery. The Read Me explains that running
out of time restarts the level and costs a pea shooter.
