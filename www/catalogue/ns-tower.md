---
id: ns-tower
kind: game
title: NS-TOWER
summary: Charge and release jumps to climb a tower of moving platforms.
developer: NAGI-P SOFT
year: 1997
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 7ee2672626e3098f39e9376bcb60d3e99d523947
    architecture: 68k
    environment: >-
      Bounded native v0.95.2 with the merged SetPort correction, original English
      2.5 executable and explicit 68K slice, 800-by-600/8-bit display. Return
      dismisses the unchanged shareware notice; an ordinary click starts New Game.
      Holding Space fills the power meter and releasing it jumps. Longer charges
      reach a raised platform and scroll the tower upward. A fresh replay passes
      six measured pixel assertions at 1366 frontend / 1966 guest ticks with zero
      exhausted frames. A matched no-Space run stays on the bottom floor at the
      same clock. All eight original forks match independent extraction.
      Title-dialog remnants remain outside the playfield. Floor-counter progression,
      ordinary browser/public gameplay, saved scores, long runs and audio remain
      unverified. Browser launch remains disabled pending approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4480
runtime:
  executable_path: NS-TOWER 2.5/NS-TOWER
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://www.nagi-p.com/v1/files/ns-tower-25.sit.hqx
    expected_sha256: 50e9a1fd942cebf9d3a549fee19b590b0248e009d1c3cfcd613ae4ef600d3d51
    expected_size: 829529
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.nagi-p.com/v1/files/ns-tower-25.sit.hqx
    - https://www.nagi-p.com/v1/eng/nstower.html
    - https://github.com/benletchford/systemless/issues/4480
    license: Original unchanged-package distribution permission
    rights_holder: NAGI-P SOFT / Akihiko Kusanagi
    permission: >-
      The original English 2.5 Read Me First and the matching author's Macintosh
      information page permit freely distributing the unchanged package. Other
      media such as CD-ROM require contact. This free online distribution preserves
      all four original files, documentation, registration program and the US$10
      shareware reminder without bypassing registration.
    notes: >-
      Downloaded directly from the author's English 2.5 download link. Production
      BinHex and StuffIt decoding matches independent extraction for all four files
      and eight forks. The game has four CODE resources, an empty data fork and no
      CFM or PEF slice; this exact version is 68K-only.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/ns-tower/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4480
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Lossless crop of inspected original gameplay after jumping to a raised
      platform, including the HUD and tower without host UI, window title or
      Classic menu bar. Title-dialog remnants remain; no redraw or guest patch.
references:
- https://www.nagi-p.com/v1/eng/nstower.html
---

![NS-TOWER character above a raised platform](incoming/ns-tower/gameplay.png)

Dismiss the original shareware notice and choose New Game. Hold Space or the
mouse button to charge a jump, then release it. Longer charges jump higher,
but holding too long drains the power meter. Time jumps to land on platforms
and climb the tower.

This is the complete original English **NS-TOWER 2.5** shareware package,
downloaded directly from its author, including Read Me First and Register.
The original US$10 registration reminder is retained. This version is 68K-only.

Bounded native testing verifies charging, jumping and reaching a raised
platform. Some title-dialog pixels remain outside the tower. Browser launch
is pending testing and approval. Long runs, saved scores and audio remain
unverified.
