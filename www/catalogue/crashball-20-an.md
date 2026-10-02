---
id: crashball-20-an
kind: game
title: CrashBall 2.0An
summary: >-
  Dodge and destroy bouncing bubbles within a 45-second limit in this colorful
  arcade game.
developer: Cédric Protti
publisher: Cédric Protti
year: 1998
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
      The unchanged unregistered Info-Mac package opened in 68K Systemless. After
      dismissing the startup notice, File > Play (normal) reached an active Level 1 with
      its timer, lives, bubbles, and obstacles visible. Moving the mouse relocated the
      green player; a click fired a visible projectile. The release browser fetched the
      same archive once and reproduced mouse movement and firing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3888
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 7d2ead7eb892b936cbed1cdf25e788c152ac4a82c25f30fb180c2badf5087949
    size_bytes: 1031751
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/crashball-20-an.hqx
    rights_holder: Cédric Protti
    permission: >-
      The bundled Read me explicitly permits free distribution of this unmodified
      CrashBall 2.0An shareware edition with every related file, including through online
      services and CD-ROM collections. It states that the registered 2.0Ae edition is
      personal and may not be distributed. This is the unchanged ten-level 2.0An package.
    notes: >-
      Original 1,031,751-byte Info-Mac BinHex/StuffIt package; SHA-256
      7d2ead7eb892b936cbed1cdf25e788c152ac4a82c25f30fb180c2badf5087949.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 36b6ef23250d7eb5c2e728fd12d6839c24abfe3be1793d79ed1b4f14cd36e994
    size_bytes: 4034
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3888
    permission: >-
      Fresh Systemless gameplay capture from the unchanged unregistered package.
      Underlying artwork remains its owner's property.
    notes: >-
      640-by-460 game-content crop at (80,80) from an 800-by-600 Systemless
      framebuffer on Level 1 after the player moved and fired.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/crashball-20-an.hqx
---

## Clear the bubbles

![CrashBall Level 1 with a projectile in flight](https://assets.systemless.org/catalogue/media/sha256/36/36b6ef23250d7eb5c2e728fd12d6839c24abfe3be1793d79ed1b4f14cd36e994.png)

Dismiss the unregistered-version notice with **OK**, then choose
**Play (normal)** or **Play (easy)** from the **File** menu. Move the
mouse to steer the green player and click to fire. Clear every bubble
before the timer expires; the shareware edition includes ten levels.
