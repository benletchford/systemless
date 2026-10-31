---
id: siege
kind: game
title: Siege
summary: Surround opponents' cards in NAGI-P SOFT's colourful strategy game.
developer: NAGI-P SOFT / Akihiko Kusanagi
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-30"
    tester: Catalogue maintainer
    systemless_version: 7cfcd9bd06c4587b3c65c329048c09053f816181
    architecture: 68k
    environment: >-
      Native 68K at 8-bit depth with the original shareware archive. The notice was
      dismissed, a new game dealt, and a player card placed. Three pixel assertions
      passed; a matched no-click control retained the card in hand. A subsequent run
      recorded non-silent audio. This is bounded gameplay and not audio fidelity or
      an independent emulator comparison.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5345
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 970d8ea1019116aa89902dd5cd750b83daeb6a55
    architecture: 68k
    environment: >-
      Local Chrome release preview with original archive supplied by local interception.
      The original notice and player turn were inspected; a 20-second sample measured
      60.166 guest ticks and 60.416 worker replies per second following the generic
      pacing fix merged in PR 5363. A separate local replay completed round one and
      entered round two. Managed hosting and hosted browser approval remain pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5362
runtime:
  executable_path: Siege 1.1/Siege
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://www.nagi-p.com/v1/files/siege-11.sit.hqx
    expected_sha256: 9de69268259acab7d1abe1e8862d131fae06f7ea1115180ec3ba75697dae31bd
    expected_size: 629668
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.nagi-p.com/v1/eng/siege.html
    - https://github.com/benletchford/systemless/issues/5345
    license: NAGI-P SOFT unchanged-package shareware distribution terms
    rights_holder: NAGI-P SOFT
    permission: >-
      The author permits free distribution of the unchanged complete package, with
      contact required for other media such as CD-ROM. The original shareware notice,
      documentation, game and registration program are retained.
    notes: >-
      Independent unar extraction matched all four files' data and resource forks
      against the StuffIt decode. The game has 68K CODE resources and no cfrg; the
      native 68K route was exercised. Registration and package files are unchanged.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/siege/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://www.nagi-p.com/v1/eng/siege.html
    - https://github.com/benletchford/systemless/issues/5345
    license: NAGI-P SOFT unchanged-package shareware distribution terms
    rights_holder: NAGI-P SOFT
    permission: Original game content accompanying the unchanged permitted package.
    notes: >-
      Original native gameplay after a player card placement, cropped to the 640 by
      440 game-content surface without desktop, menu bar or window frame.
references:
- https://www.nagi-p.com/v1/eng/siege.html
- https://www.vector.co.jp/vpack/filearea/osx/game/table/card/
- https://github.com/benletchford/systemless/issues/5345
- https://github.com/benletchford/systemless/issues/5362
---

Play the original **Siege 1.1**, a card strategy game by NAGI-P SOFT. Place cards
next to those already on the board and try to enclose spaces belonging to your
opponents. The computer plays three hands; the round ends when no cards can be
played.

The original shareware registration notice remains. Native card placement and one
local browser round are verified; hosted browser qualification is pending.

![Siege round in progress](incoming/siege/gameplay.png)
