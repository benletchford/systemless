---
id: frequon-invaders
kind: game
title: Frequon Invaders
summary: >-
  Annihilate attacking particles by steering your wave into destructive
  interference in this Fourier-space arcade game.
developer: Arch D. Robison
year: 2001
architectures:
- ppc
default_architecture: ppc
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 9dde754438eddce4d35f0bfc0a02a1d92b088483 native; runtime merged in b912638f
    architecture: ppc
    environment: >-
      Complete unchanged original; exact executable; default PowerPC/25 MHz,
      800x600/depth 8. Re-select original checked Medium display as documented, retain Auto
      Gain and start normal Begin Game with Command-B. No Training, Peek or smaller
      display. Pointer movement from (106,42) toward the visible attacker at (214,274)
      annihilates it and lights the first binary score bit; equal-clock omitted-movement
      control remains at zero. Both cases pass eight measured causal pixels and nine exact
      own-run image comparisons, frontend 1808/guest 1787 ticks, zero exhausted frames.
      Pre-input captures match; actual final captures inspected. Own-run consistency
      is not hardware accuracy. Both cases also match exactly on dffc7668 PEF relocation
      candidate. Baseline binary SHA-256
      b2aada24f38cf1f555f6b76939bfee827d3a1656e3c6915ca1af92e8a408b8ff. Browser input/pacing, complete games, persistence, audio
      fidelity and public release remain unverified; browser launch stays disabled pending
      testing.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5806
runtime:
  executable_path: Frequon Invaders 1.1ƒ/Frequon Invaders
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 2de7b9691b7140f833ee738938cd0352f0d25643e40b761cf0275ffa3010f4b3
    size_bytes: 74973
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/frequon-invaders.hqx
    - https://github.com/benletchford/systemless/issues/5806
    license: Original unchanged distribution grant with accompanying README
    rights_holder: Arch D. Robison
    permission: >-
      Complete 8071-byte original README, revised February 15, 2001, explicitly
      permits free distribution unchanged with the README. Preserve the complete
      application, documentation, forks, Finder metadata and notices.
    notes: "Independent extraction matches production packing: two files, four forks, two Finder records; strict KPK 116334 bytes, SHA-256 20d11acdd33ae830992e71edac27f9e7406b7e5600a54fb54a8c387453d302ff. Original PowerPC-only application; no external runtime supplied."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d54850debbd273832801618d987b1a2e82a1d4243e6c57b880fff81a0a62b8cd
    size_bytes: 87743
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5806
    rights_holder: Arch D. Robison
    permission: Fresh content-only capture from the unchanged distributable original.
    notes: >-
      Exact crop (16,39)-(515,474), no resampling; first point after ordinary mouse
      movement.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/frequon-invaders.hqx
- https://github.com/benletchford/systemless/issues/5806
---

![Fourier waves after the first annihilation](https://assets.systemless.org/catalogue/media/sha256/d5/d54850debbd273832801618d987b1a2e82a1d4243e6c57b880fff81a0a62b8cd.png)

Choose Medium from Display, then Begin Game from File (Command-B). Move the mouse
through the coloured wave field to find destructive interference and annihilate
attacking Frequons. Pastel marks at the left show attackers; six binary lamps show
your score. Keep Auto Gain checked while learning. The original README explains
the waves and includes a separate unscored Training mode.
