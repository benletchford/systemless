---
id: tic-tac-tom
kind: game
title: TicTacTom
summary: >-
  Place X marks and play against the computer in Tom Ward's Macintosh tic-tac-toe
  game.
developer: Tom Ward
year: 1999
architectures:
- 68k
default_architecture: 68k
launch_enabled: false
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1858f837 native baseline; browser qualification pending
    architecture: 68k
    environment: >-
      Complete unchanged original archive; exact TicTacTom v1.0/TicTacTom executable.
      68K/default25MHz/default800x6008-bit/original partition; original
      difficulty,sound and other settings retained. File → New Game,then click the top-left and
      bottom-right cells. X marks both cells and the computer makes two O replies;
      equal-frontend-clock control omits only those button edges and stays empty. Both repeats
      pass four causal cell pixels and seven exact prior captures at678 frontend ticks
      and guest TickCount1256,zero exhausted frames. Actual captures inspected. Own
      references establish consistency,not hardware accuracy. Original normal3x3 mode
      retained;3D mode and full rounds unverified. Native binary
      SHA-256148f46937353de4e3b322b2c14595204579a07b288754c853003bbc0b4a47cc1. Full games,other
      modes,persistence,audio fidelity and released public play unverified. Launch disabled pending
      actual hosted-original browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5864
runtime:
  executable_path: TicTacTom v1.0/TicTacTom
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 4f446b05f1cec70c3e7d5775889842ddb3df61e47630cd96598ad0554cd098bc
    size_bytes: 67982
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/tic-tac-tom-10.hqx
    - https://github.com/benletchford/systemless/issues/5864
    license: Original unchanged-distribution grant
    rights_holder: Tom Ward
    permission: >-
      The complete original readme expressly permits distribution provided Tom Ward
      receives credit, the readme accompanies the program and the program is unchanged.
      Preserve the original warranty disclaimer,credit and all notices with the
      complete unchanged archive. No game modification or registration bypass.
    notes: >-
      Full1424-byte original readme reviewed; version1.0 dated7August1999. Complete
      three-path package retains original application,readme and folder icon. Strict
      verification matches all three paths,six forks and Finder records. Production pack
      100195bytes,SHA-256405b42b9852b1720cff4e04c0f4d12037ab17ee449c7e79af20f1ce94a33a4e0. No runtime added or installed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 62ab43bb68c32b22391d19dfd84d284197854f38d472dc30ec3eaa6da9feefeb
    size_bytes: 2746
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5864
    permission: Original gameplay capture of the unchanged freely distributed game.
    notes: >-
      Exact native content crop [324, 113, 474, 263] without resampling; excludes
      desktop,menu bar and window chrome. Human move and computer reply visible.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/tic-tac-tom-10.hqx
- https://github.com/benletchford/systemless/issues/5864
---

![Original TicTacTom gameplay](https://assets.systemless.org/catalogue/media/sha256/62/62ab43bb68c32b22391d19dfd84d284197854f38d472dc30ec3eaa6da9feefeb.png)

Choose File → New Game, then click an empty square to place an X. The computer plays O. The original game also offers a 3D mode; the normal 3×3 mode is the verified configuration. The complete original program and readme are preserved.
