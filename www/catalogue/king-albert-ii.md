---
id: king-albert-ii
kind: game
title: King Albert II
summary: Build four foundations from ace to king in this colour solitaire game, using nine tableau columns
  and a seven-card reserve.
developer: Ricardo Rozemberg
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: fc327b1bd49f148a45356a37b1d2f25aac4f5c96 native; runtime included in b4f88657
      master
    architecture: 68k
    environment: 'Historical baseline before #5798: unchanged original, exact executable, default 25 MHz/800x600/depth8.
      At frontend 240/guest840 the board and placeholders appear but dealt card faces are absent; zero
      exhausted frames. Actual capture inspected. This is startup evidence only, not playable approval.'
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3982
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 9dde754438eddce4d35f0bfc0a02a1d92b088483 native MapRgn candidate
    architecture: 68k
    environment: 'Complete unchanged original; exact executable; default 25 MHz/800x600/depth8. The #5798
      candidate repairs complex MapRgn masks and displays the dealt cards. Ordinary exposed diamond ace
      drag (298,245) to foundation (86,80) moves the ace and reveals a spade; equal-clock no-button control
      retains the ace in its column and empty foundation. Fresh repeats each pass six causal pixels and
      two exact prior captures, frontend 368/guest 968, zero exhausted frames. Actual images inspected;
      pre-input captures match. Own-run consistency is not hardware accuracy. Native binary SHA-256 b2aada24f38cf1f555f6b76939bfee827d3a1656e3c6915ca1af92e8a408b8ff.
      Runtime prerequisite #5798 is merged as b912638f7bdd236bb2a03d9aa67e340981e427b0 with identical
      runtime content. Full solves, persistence, audio fidelity and released public play remain unverified;
      hosted-original browser evidence follows below. A separate ordinary Undo restores the full initial
      image with zero differing pixels.'
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3987
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: fafe2deb5681cd3c1b32e705ca360cd7af87acdc private browser on 9dde7544 runtime
    architecture: 68k
    environment: Fresh Chromium isolated worker/WebGL profiles and SharedArrayBuffer; exact original executable,
      default 68K/25 MHz/800x600/depth8. Ordinary audio gesture precedes each unchanged 20-second gameplay
      sample. In inspected original deal 418036440, a legal drag moves the exposed red ten of diamonds
      from column one onto the black jack of spades in column four, leaving the source hole. Fresh no-button
      control retains its independently shuffled deal 418036373. Actual before/final captures viewed;
      no same-deck or browser determinism claim. Active/control steady guest 61.0203/60.0011 and host
      60.0033/60.0011 per second; maximum worker frames 32.6/3.7 ms, minimum audio queues 117.7/133.1
      ms. All unchanged pacing gates pass. Game ID, worker, 68K,25 MHz, full private source stamp, isolation
      and shared memory checked. Actual original archive and both main/worker Wasm HTTP 200 response bodies
      match independent byte/hash pins. Wasm 17,885,492 bytes, SHA-256 4afd18decea8571157c5289e814149ae34f2b640f2c3e35c6d46aed50168f55e.
      No captured lifecycle/network errors. Rates are steady snapshots, not whole-game averages. Runtime/browser
      implementation content matches merged b912638f7bdd236bb2a03d9aa67e340981e427b0, unchanged through
      531c0af4 master. Full solves, persistence, audio fidelity and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3987
runtime:
  executable_path: King Albert II v1.0/King Albert II v1.0
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 5746cc66fad3a0c9a29fdef13ea84882b1f258fc8f77fd1ef8e2bd632b7a3121
    size_bytes: 58109
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/crd/king-albert-ii-10.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/crd/00crd-abstracts.txt
    - https://github.com/benletchford/systemless/issues/3987
    license: Original distribution terms with accompanying documentation
    rights_holder: Ricardo Rozemberg
    permission: The complete original July 21, 1996 ReadMe explicitly permits distribution as long as
      that document accompanies the game. Preserve the complete unchanged application, ReadMe, all forks/Finder
      metadata and original $10 shareware notice. Full 5,761-byte ReadMe reviewed. No registration, date,
      shuffle or game code change.
    notes: 'Independent extraction matches production packing: two paths, four forks, two Finder records;
      strict KPK 78,010 bytes, SHA-256 f640bb15e1a4ff4af90279ff3aed886158b963695b6de27a4c5de8797f19e7a6.
      Original 68K application; no external runtime supplied.'
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 897332d755be60b136ed45b7d3477053357a46a265aeb741bdcc492ce053cc39
    size_bytes: 19607
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3987
    rights_holder: Ricardo Rozemberg
    permission: Fresh content-only capture from the complete unchanged distributable original.
    notes: Exact crop (0,38)-(568,442), 568x404; no resampling. Native candidate after an ordinary legal
      foundation move; excludes menu and title bar.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/crd/king-albert-ii-10.hqx
- https://github.com/benletchford/systemless/issues/3987
- https://github.com/benletchford/systemless/pull/5798
launch_enabled: true
---

Drag exposed cards to build tableau columns down in alternating colours. Build each foundation from ace to king in one suit. Any card in the reserve may be used, and any card may fill an empty tableau column. Undo and Redo let you review moves. The original $10 shareware notice remains in the package.
