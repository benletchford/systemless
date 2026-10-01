---
id: fly-swatter
kind: game
title: Fly Swatter 2.0
summary: Chase a changing series of flies with the mouse before they escape.
developer: Victor Franco
publisher: Victor Franco
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
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The unchanged author-submitted Info-Mac freeware archive opened in 68K
      Systemless. File > New Game opened a live fly window. After one fly moved on, a mouse
      click struck the next fly and the score changed from 0 out of 1 to 1 out of 2 while
      a new fly appeared.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3768
  - date: "2026-10-02"
    tester: Catalogue maintainer
    systemless_version: 0.72.0 + release browser build
    architecture: 68k
    environment: >-
      Chrome loaded the local release build and fetched the unchanged archive once.
      File > New Game opened a live fly window. The fly counter advanced; a real browser
      mouse click struck a visible fly and raised the score to 1 out of 7 while a new
      fly appeared. A five-second active sample measured about 60 host frames and 60
      guest ticks per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3768
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 601990f0a162b9664be462482154281759209e755b9f1dc0c512edbb3931d18c
    size_bytes: 23908
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    - >-
      https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/fly-swatter.hqx
    rights_holder: Victor Franco
    permission: >-
      Victor Franco submitted this original freeware package to Info-Mac. Its bundled
      Read Me calls the complete game freeware, invites readers to try and keep it,
      and identifies Info-Mac as a distribution venue. This entry preserves the complete
      unchanged archive and Read Me.
    notes: >-
      Original 23,908-byte BinHex/StuffIt archive, SHA-256
      601990f0a162b9664be462482154281759209e755b9f1dc0c512edbb3931d18c.
      The promoted public object was fetched and confirmed byte-for-byte identical
      to the original download on 2026-10-02.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 15c442c3137387cb94222520b167551d7e71cd38501ff41474e89c6b7e539040
    size_bytes: 2168
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3768
    permission: >-
      Fresh Systemless gameplay capture from the unchanged freeware game. Underlying
      artwork remains its owner's property.
    notes: >-
      Exact 580-by-400 game-content crop at (110,110) from an 800-by-600 Systemless
      framebuffer after a successful swat; score 1 out of 2. The promoted public
      PNG matched its 2,168-byte SHA-256 source on 2026-10-02.
references:
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- >-
  https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/fly-swatter.hqx
---

## Swat before they fly away

![Fly Swatter after a successful swat](https://assets.systemless.org/catalogue/media/sha256/15/15c442c3137387cb94222520b167551d7e71cd38501ff41474e89c6b7e539040.png)

Choose **New Game** from the **File** menu. Move the mouse over each fly and
click to swat it before it leaves. A hit raises your score; a missed fly
counts against it.
