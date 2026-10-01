---
id: bachman
kind: game
title: Bachman
summary: Clear dots from a three-dimensional maze while avoiding the pursuing enemies.
developer: Ingemar Ragnemalm
publisher: Ingemar Ragnemalm
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged Info-Mac archive opened in 68K Systemless. Command-B started
      Level 1. At the same tick, holding Right moved Bachman along the maze and raised the
      score to 6; a no-input replay stayed at 0.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3747
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once.
      Command-B opened Level 1; holding Right during active play moved Bachman across
      the maze and raised the score to 6. A five-second sample advanced at about 60 guest
      ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3747
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 7d9dcbcfac16bec7f8818710c891515ec9429c2ad1b107782c44d432de15391b
    size_bytes: 310177
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/bachman-205u.hqx
    - https://www.lysator.liu.se/~ingemar/games/news.html
    rights_holder: Ingemar Ragnemalm
    permission: >-
      The bundled Bachman documentation says this edition without the editor is free
      for all noncommercial use and may be passed along unchanged; it forbids sale for
      profit without written permission. The author's games news later also identifies
      Bachman as freeware. This entry uses the complete unchanged archive with its
      documentation.
    notes: >-
      Original 310,177-byte BinHex/StuffIt archive, SHA-256
      7d9dcbcfac16bec7f8818710c891515ec9429c2ad1b107782c44d432de15391b.
      The public object was fetched back and matched the original hash and size.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 051fb9198502fe71180b0c231edbb17df1842f94f509921758c1eb98b2a8420d
    size_bytes: 8370
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3747
    permission: >-
      Fresh Systemless gameplay capture from the author's original freeware release
      for this catalogue entry. Underlying artwork remains its owner's property.
    notes: >-
      Exact 518-by-307 game-content crop at (132,138) from an 800-by-600 Systemless
      framebuffer after moving Bachman right and collecting dots. The public
      media object was fetched back and matched the submitted hash and size.
references:
- >-
  https://ftp.zx.net.nz/pub/mirror/ftp.funet.fi/pub/mac/info-mac/game/arc/bachman-205u.hqx
- https://www.lysator.liu.se/~ingemar/games/news.html
---

## Clear the maze

![Bachman moving through the Level 1 maze](https://assets.systemless.org/catalogue/media/sha256/05/051fb9198502fe71180b0c231edbb17df1842f94f509921758c1eb98b2a8420d.png)

Choose **Begin** from the **Game** menu or press **Command-B**. Move Bachman
with the arrow keys or numeric keypad to collect dots while avoiding enemies.
The **Control** menu offers alternate movement settings.
