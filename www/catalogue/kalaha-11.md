---
id: kalaha-11
kind: game
title: Kalaha 1.1
summary: Sow balls around a colourful mancala-style board against the computer.
developer: Joachim Kulla
publisher: Joachim Kulla
year: 1998
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 97ec1e637d085b447685cc055e4f0e9e8a862b65
    architecture: 68k
    environment: >-
      Release-mode local Chrome preview, normal DOM keys and stationary clicks,
      regular WebAssembly worker, 8 MHz pacing. Command-B filled six balls per pit;
      three human clicks completed an extra turn and two AI replies. Inspected settled
      boards conserved all 72 balls. Preview archive requests used independently
      hash-verified original bytes. Separately, the deployed v0.84.1 browser benchmark
      fetched and hash-checked the exact Systemless-hosted archive without interception
      and reproduced bounded gameplay. Full match, all levels, saves and audio unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3945
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 97ec1e637d085b447685cc055e4f0e9e8a862b65
    architecture: ppc
    environment: >-
      Same release-mode local Chrome preview and intact FAT archive, selected using
      the normal PowerPC control. Canvas confirmed worker=true and architecture=ppc.
      Command-B began the game; stationary clicks completed a human extra turn and
      two computer replies on inspected boards. Independently hash-verified original
      bytes supplied to local preview archive requests. Full match, levels, saves
      and audio unverified; default 68K hosted loading is checked separately.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3945
  - date: "2026-10-03"
    tester: Catalogue maintainer
    systemless_version: 1cd9de170746430e9395b69811aac592db1b4c64
    architecture: 68k
    environment: >-
      Native public-API harness only, with the FAT archive explicitly loaded as 68K,
      an 800-by-600 8-bit display, and 32-bit addressing. Command-B began a six-ball
      game. A human extra-turn move and two AI replies completed, each conserving all 72
      balls. An opponent-pit click was ignored; New Game and About opened and the About
      dialog dismissed. Two clean runs reproduced all eleven captures. No
      release-browser run, complete match, or audio test has been performed. This record covers native testing only;
      later browser verification is recorded separately.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3945
runtime:
  runtime_pacing:
    cpu_mhz: 8
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: e30d98a0b2ecc8eeb50886d6ce5ba06335b1877a92746903f70dca45b6dd62bc
    size_bytes: 398473
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/kalaha-11.hqx
    rights_holder: Joachim Kulla
    permission: >-
      The bundled Read Me permits unchanged distribution with all accompanying files,
      provided it is not distributed for profit. Distribution for profit requires the
      author's permission. This complete original archive preserves the application,
      Read Me, and icon resource; no commercial permission is asserted.
    notes: >-
      Original 398,473-byte Info-Mac BinHex/StuffIt package. The Read Me dates
      version 1.1 to November 1998 and identifies the application as FAT. Both 68K CODE
      resources and a PowerPC code fragment are present; the original native record tested the 68K executable.
      Shareware permits a 30-day evaluation, then asks the user to register or stop
      using the game.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b83fa74a209e51b097980fc9b8adb28bcb11ec76fe4f709bae1b6ea752f54800
    size_bytes: 41356
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3945
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original shareware package
      for this non-profit catalogue entry. Underlying game artwork remains its owner's
      property.
    notes: >-
      Authentic native framebuffer after a human extra turn and two AI replies. Exact
      556-by-341 content crop at (121,148) from the 800-by-600 framebuffer; no
      rescaling or retouching. Host framing and the Classic Mac menu and window title bars are
      excluded. This is not a release-browser capture.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/kalaha-11.hqx
---

## Sow, capture, and collect

![Kalaha after native human moves and two computer replies](https://assets.systemless.org/catalogue/media/sha256/b8/b83fa74a209e51b097980fc9b8adb28bcb11ec76fe4f709bae1b6ea752f54800.png)

Choose the ball count and difficulty in **Balls** and **Level**, then press
**Command-B**, or choose **Game → Begin**, to fill the board. Click one of your
six lower pits to sow its balls counter-clockwise, skipping the computer's
store. Ending in your own store earns another turn. The original Read Me
explains this game's capture rule, which differs from some mancala variants.

Joachim Kulla's original shareware release allows a **30-day evaluation**,
after which its Read Me asks the player to register or stop using it. The
unchanged package may be redistributed with all accompanying files only
without profit unless the author grants permission for commercial distribution.

## Verification status

Native checks covered a six-ball game, an extra human turn, two computer
responses, an invalid opponent-pit click, New Game, and the About dialog. The
screenshot comes from the 68K native run. Release-mode browser checks now cover
Begin, stationary human moves, the extra turn and AI responses in both 68K
and PowerPC worker players. The exact hosted archive also loads in the deployed browser
benchmark. Full-match completion, all difficulty settings, saving and audio
remain unverified.
