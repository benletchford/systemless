---
id: gridz
kind: game
title: Gridz Demo 1.2
summary: >-
  Claim a shifting three-dimensional board in Green Dragon Creations' strategy
  demo.
developer: Green Dragon Creations, Inc.
publisher: Green Dragon Creations, Inc.
year: 1997
architectures:
- 68k
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: boots
  verified:
  - date: "2026-10-09"
    tester: Catalogue maintainer
    systemless_version: 0.83.0 + absolute boot-volume lookup fix
    architecture: ppc
    environment: >-
      Headless replay of the intact installer passes the display prompt, opens New
      Game and player setup, and reaches a live board. A board click visibly changes the
      board compared with an equal-duration idle run. Sustained play, save/restart, and
      extended browser gameplay remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4227
  - date: "2026-10-09"
    tester: Catalogue maintainer
    systemless_version: 0.83.0 browser build from promoted PR head 35083fa004ac
    architecture: ppc
    environment: >-
      Isolated headless Chrome tested staged page resources at the production
      origin, while fetching the actual Systemless-hosted installer without archive
      interception. Browser hashing confirms the complete 5,453,841-byte archive
      and recorded SHA-256. The display prompt, title, New Game, player setup,
      loading screen, live board, and a board click were observed. This is bounded
      browser interaction; sustained gameplay and save/restart remain unverified.
      It does not claim that the staged route was deployed on the live site.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4227
  - date: "2026-10-09"
    tester: Catalogue maintainer
    systemless_version: 0.83.0 + absolute boot-volume lookup fix
    architecture: 68k
    environment: >-
      After the absolute resource-path fix, the original installer reaches the title
      animation instead of the missing-resource alert. The title rendering remains
      visibly corrupted; 68K gameplay is unverified.
    status: broken
    evidence: https://github.com/benletchford/systemless/pull/4229
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: a3eb2f24f75f01944dd7f7816ec7720e5250023e57bd7198c49e2d2abb1567a9
    size_bytes: 5453841
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/gridz
    - https://static.classicmacdemos.com/demos/gridz/README.txt
    - https://github.com/benletchford/systemless/issues/4227
    license: Gridz demo installer distribution permission
    rights_holder: Green Dragon Creations, Inc.
    permission: >-
      The bundled README expressly permits freely distributing the demo in its
      installer form and requires written consent for other use of its parts. Preserve the
      complete unchanged installer and its terms.
    notes: >-
      Original 5,453,841-byte BinHex archive matches the previously catalogued
      installer hash. Its bundled 1,043-byte README is byte-identical to the linked original
      text, with SHA-256
      056737c012269f6cfbad05e77affbaa60d66e0f28394b5a7e328913b1d72a1c3.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d68bb1fa60e2c0f10c2e3020a46c9c20286e7744b7944116e157086f88ecde46
    size_bytes: 69083
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4227
    permission: >-
      Original Systemless gameplay capture made for this catalogue contribution.
      Underlying game artwork remains Green Dragon Creations' property.
    notes: >-
      PowerPC board after a scripted click, captured from the exact intact installer
      and cropped to its 800 by 580 content surface below the menu bar.
references:
- https://classicmacdemos.com/gridz
- https://github.com/benletchford/systemless/issues/4227
---

Gridz combines territorial strategy with a three-dimensional board. This entry
preserves the complete version 1.2 demo installer, including its original terms
and data.

The PowerPC version reaches a new game and accepts a board click in bounded
native and headless browser testing against the hosted installer. Longer play
and save/restart still require validation. The
68K version currently has corrupted title rendering. Browser launch remains
disabled while qualification continues.

![Gridz PowerPC board after a click](https://assets.systemless.org/catalogue/media/sha256/d6/d68bb1fa60e2c0f10c2e3020a46c9c20286e7744b7944116e157086f88ecde46.png)
