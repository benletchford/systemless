---
id: smart-cookies-201
kind: game
title: Smart Cookies 2.0.1
summary: Clear a colorful board by removing connected groups of matching cookies.
developer: Georges Gyory
publisher: Georges Gyory
year: 1998
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.73.0 + deterministic play runner and release browser build
    architecture: 68k
    environment: >-
      The unchanged unregistered Info-Mac package opened in 68K Systemless.
      Dismissing its title picture revealed a full cookie board. Clicking a connected group
      removed cookies and raised the score from 0 to 16. The release browser fetched the
      same archive once and reproduced the board change and score increase.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3895
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 1d7c62f32bdd48e98cc2a069844473c1b8f0372bf8e307a434776a26cb3098a4
    size_bytes: 243932
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://info-mac.org/viewtopic.php?t=11456
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/smart-cookies-201.hqx
    rights_holder: Georges Gyory
    permission: >-
      In the author-submitted Info-Mac description, Georges Gyory expressly
      authorizes anyone to distribute the unregistered and unaltered shareware version. This is
      the complete unchanged original archive.
    notes: >-
      Original 243,932-byte Info-Mac BinHex/StuffIt package; SHA-256
      1d7c62f32bdd48e98cc2a069844473c1b8f0372bf8e307a434776a26cb3098a4.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 75ed4760f3378c9a52689ec3a2b7acf31444999b86f21b8d72db54c82a1f1d8c
    size_bytes: 41605
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3895
    permission: >-
      Fresh Systemless gameplay capture from the unchanged unregistered package.
      Underlying artwork remains its owner's property.
    notes: >-
      514-by-343 game-content crop at (143,129) from an 800-by-600 Systemless
      framebuffer after one connected group was removed and the score reached 16.
references:
- https://info-mac.org/viewtopic.php?t=11456
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/smart-cookies-201.hqx
---

## Clear the cookie board

![Smart Cookies board after a scoring move](https://assets.systemless.org/catalogue/media/sha256/75/75ed4760f3378c9a52689ec3a2b7acf31444999b86f21b8d72db54c82a1f1d8c.png)

Click the title picture to open the board. Click a connected group of
same-color cookies to remove it and score. The **Setup** menu offers board
size and color options; aim to empty the board.
