---
id: par-pyramid
kind: game
title: Par Pyramid
summary: >-
  Remove exposed cards totalling thirteen in this colour Pyramid Solitaire game
  with optional Par Pyramid scoring.
developer: Grayson Myers
year: 1996
architectures:
- 68k
- ppc
default_architecture: ppc
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 9dde754438eddce4d35f0bfc0a02a1d92b088483 native baseline
    architecture: 68k
    environment: >-
      Original 68K slice, default25MHz/800x600/depth8, reaches the title at
      frontend240/guest840 ticks with zero exhausted frames; actual image inspected. Startup
      diagnostic only. No 68K gameplay or browser approval; both original CPU slices
      remain in the unchanged archive.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5802
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 722b0c8e28fc15e4fe6214ff64dc11d441569cad native background-pattern candidate
    architecture: ppc
    environment: "Complete unchanged original, exact executable and original PowerPC/default 25MHz/800x600/depth8. EraseRect now draws the original textured background. Resource/API trace links GetPixPat128 handle03023A00 to the live bkPixPat used by EraseRect on the main and offscreen board ports. Command-N deals; click438,390 removes the exposed king and reveals the card behind it. Equal-clock no-button control retains the king. Both repeats pass fourteen checks: eight causal card pixels, two texture pixels and four exact prior own-run captures, frontend/guest1028 ticks, zero exhausted frames. Actual images inspected; pre-input images match. Own-run consistency is not a hardware oracle. Score remains zero in both; scoring and full games remain unverified. Native binary SHA-256 49f56faf96eb1ad90595fb4ff33995e4a26d1d8700c2aef893ba5a277e92058c. Includes CopyPixMap c535e205 and the background fix merged as e2930db7f674dae3b1f467db66c92c655aa84e4b, whose full source tree is identical to this tested candidate and d7de5122. Browser evidence follows below. Full games, scoring, persistence, audio fidelity and released public play remain unverified. The earlier white-background case is retained in issue5813."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5813
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 9cfc3dec74fd81e4d903d64dd9b763a06265d987 private browser on 722b0c8e runtime
    architecture: ppc
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5811
    environment: >-
      Fresh Chromium 156/Linux aarch64 profiles, isolated worker/WebGL and
      SharedArrayBuffer; original PowerPC/default 25 MHz/800x600/depth 8.
      Ordinary audio gesture outside the app window, then Command-N deals a
      board with the original texture. During the unchanged 20-second active
      sample, a trusted click at (438,390) removes the exposed king and reveals
      the card behind it. Equal-clock no-button pointer control retains the
      king. Actual before/final captures inspected. Score remains zero in both;
      scoring, full games, persistence, audio fidelity, 68K browser play and
      public released play remain unverified. Active/control steady guest and
      host rates are 59.9972 per second in both; maximum worker frames 27.2/21.7
      ms, minimum audio queues 123.1/128.9 ms. All unchanged pacing gates pass;
      no captured lifecycle/network errors. These are steady snapshots, not
      whole-game averages. Actual game ID, architecture, 25 MHz, worker, source
      stamp, isolation and shared memory checked. Original archive and both
      main/worker Wasm HTTP 200 response bodies match independent integrity
      pins. Wasm 17,921,223 bytes, SHA-256
      7184fe96a54b04324923fa49505abc337ec55303f4f4af5641b58029e73d5ff0.
      Runtime prerequisites are merged through e2930db7f674dae3b1f467db66c92c655aa84e4b, with
      a full source tree identical to the tested native runtime722b0c8e and
      browser runtime d7de5122. Released public play remains unverified.
runtime:
  executable_path: Par Pyramid 1.1.1 Folder/Par Pyramid 1.1.1
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: cb57ec30ce4afa58db0665d778807fa4940d821145aab442bf4ceb5e6acd3861
    size_bytes: 298201
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/crd/par-pyramid-111.hqx
    - https://github.com/benletchford/systemless/issues/5811
    license: Original free nonprofit distribution grant
    rights_holder: Grayson Myers; credited artwork by Erik Jongert
    permission: >-
      Full1864-byte original README explicitly permits free distribution without
      profit. Preserve the complete application, README, all forks/Finder metadata and
      credits for backgrounds/card backs from Erik's Desktop Patterns II.
    notes: "Independent extraction matches production packing: two files, four forks, two Finder records; strict KPK671720 bytes, SHA-256 5d9895b627c55b66651467a8b671118df97fb71c4da2734d96d4c0234abc7ed5. Version year1996 is supported by original application creation/modification timestamps June2,1996. Original fat68K/PowerPC application; no external runtime or game changes."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 4ae3e616abd59cf0535e25ec96af05958c82a3db32bdca93839c36534ffa3ddb
    size_bytes: 50933
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5811
    rights_holder: Grayson Myers; credited artwork by Erik Jongert
    permission: Fresh content-only capture from the complete unchanged distributable original.
    notes: Exact crop (153,149)-(649,489), no resampling; after legal exposed-king removal.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/crd/par-pyramid-111.hqx
- https://github.com/benletchford/systemless/issues/5811
- https://github.com/benletchford/systemless/pull/5810
---

![Pyramid Solitaire after removing an exposed king](https://assets.systemless.org/catalogue/media/sha256/4a/4ae3e616abd59cf0535e25ec96af05958c82a3db32bdca93839c36534ffa3ddb.png)

Press Command-N to deal. Remove exposed cards in pairs totalling thirteen;
kings can be removed alone. The original application includes full instructions
and offers regular Pyramid Solitaire or Par Pyramid scoring.

