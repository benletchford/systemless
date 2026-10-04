---
id: centaurian
kind: game
title: Centaurian 1.2.1
summary: Fly through Zone 0, shoot enemy ships, and defend humanity in David Dobson's shareware space arcade game.
developer: David M. Dobson
publisher: David M. Dobson
year: 1996
architectures: [68k, ppc]
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-10-05"
    tester: Catalogue maintainer
    systemless_version: 0.76.1 deterministic play runner
    architecture: 68k
    environment: >-
      The original 1.2.1 fat application reaches the title screen and Zone 0
      gameplay. Matched runs show Space firing and numeric keypad 4 steering.
      A local 0.76.1 Trunk browser preview using the same staged HQX reaches
      live Zone 0 play after New Game, with no console errors observed.
      Sustained browser input and period Macintosh comparison remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4019
  - date: "2026-10-05"
    tester: Catalogue maintainer
    systemless_version: 0.76.1 deterministic play runner
    architecture: ppc
    environment: >-
      Explicit native PowerPC loading reaches Zone 0 gameplay. Matched runs
      show Space firing and numeric keypad 4 steering. A local 0.76.1 Trunk
      browser preview using the same staged HQX reaches live Zone 0 play after
      New Game, with no console errors observed. Sustained browser input and
      period Macintosh comparison remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4019
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/_Arcade/centaurian-121.hqx
    expected_sha256: dac722b842aba87068422ce95b4db4613f22bc1033135eca83ed69b7d8e18cdd
    expected_size: 1136891
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/_Arcade/centaurian-121.hqx
    - https://github.com/benletchford/systemless/issues/4019
    license: Centaurian 1.2.1 bundled license agreement
    rights_holder: David M. Dobson
    permission: >-
      The bundled 1996 license permits free redistribution of the unmodified
      software with its READ ME file, including on no-fee WWW archives.
      Registration codes must not be redistributed. This original Info-Mac package
      includes the READ ME and no registration code.
    notes: >-
      Original 1,136,891-byte BinHex/StuffIt archive, SHA-256
      dac722b842aba87068422ce95b4db4613f22bc1033135eca83ed69b7d8e18cdd.
      Both architecture replays produce pixel-identical gameplay frames to
      the separately tested 1.2.1 StuffIt package.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/centaurian/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4019
    permission: >-
      Original gameplay capture from the unregistered shareware package.
      Underlying game artwork remains its owner's property.
    notes: >-
      640x480 game-content capture from this exact Info-Mac archive running its native
      PowerPC fragment, excluding the surrounding display. SHA-256
      1e90f492b43ef160b078bb776bf03e645dc24c626f46b6e04cf9f7c65176820e.
references:
- https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/_Arcade/centaurian-121.hqx
- https://github.com/benletchford/systemless/issues/4019
---

## Defend the first contact zone

![Centaurian Zone 0 playfield](incoming/centaurian/gameplay.png)

Choose **New Game** or press **N** from the title screen. **Space** fires, and
numeric keypad **4** steers left in Absolute control mode. The in-game Ship
Controls screen lists the other keys and lets you switch steering modes.

This is the original unregistered shareware release. Its bundled license
requires registration for use beyond thirty days.
