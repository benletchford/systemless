---
id: slick-willie-iii-06
kind: game
title: Slick Willie III 0.6
summary: Dodge political foes and collect burgers in a colourful shareware arcade chase.
developer: Jim Byer and Brian Cyr
publisher: CyrBriSoft
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
    systemless_version: 0.73.0 + release browser build
    architecture: 68k
    environment: >-
      The unchanged Info-Mac package opened as a 68K application. After
      dismissing fallback-resource notices and the shareware reminder, Play
      reached a timed Level 1 board. Clicking began the chase; matched replays
      showed mouse movement move Willie from the lower right to the upper left.
      The release browser loaded the same archive once, reached active play,
      scored 20 points, and advanced from Level 1 to Level 2.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3832
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/slick-willie-iii-06.hqx
    expected_sha256: 922384be414e16eba7d9a7641714310be7b44cadbef6709d270cd08ee62d4863
    expected_size: 1200158
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/slick-willie-iii-06.hqx
    - https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    rights_holder: Jim Byer and Brian Cyr
    permission: >-
      The author's Info-Mac abstract permits free distribution provided all
      documents accompany the unchanged resources. The bundled Read Me requires
      all documents whenever the game is replicated and prohibits alteration.
      This entry preserves the original package with its Read Me.
    notes: >-
      Original 1,200,158-byte BinHex/StuffIt package, SHA-256
      922384be414e16eba7d9a7641714310be7b44cadbef6709d270cd08ee62d4863.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/slick-willie-iii-06/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3832
    permission: >-
      Fresh Systemless gameplay capture from the unchanged shareware package.
      Underlying artwork remains its owners' property.
    notes: Content-only Level 1 crop; emulator framing is excluded.
references:
- https://ftp.zx.net.nz/pub/archive/ftp.funet.fi/pub/mac/info-mac/game/arc/slick-willie-iii-06.hqx
---

## Chase the burgers

![Slick Willie III Level 1](incoming/slick-willie-iii-06/gameplay.png)

Dismiss the startup notices and shareware reminder, choose **Play**, then
click the board to begin. Move the mouse to guide Willie toward burgers and
other bonus items while avoiding the roaming opponents. The Level 1 timer
counts down as you play.
