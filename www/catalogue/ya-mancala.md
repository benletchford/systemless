---
id: ya-mancala
kind: game
title: YA-Mancala
summary: >-
  Sow stones around a carved board, capture opposing stones and outscore a
  computer opponent.
developer: Keith Rose
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 34787a02824af40e42578be50a166af783f5e7eb native; merged as c535e205
    architecture: 68k
    environment: "Complete unchanged original; exact executable; original 68K/default 25 MHz, 800x600/depth8. Preserve Medium difficulty, sound on, Slower Moves off and Less Predictable off. Game Play Game starts the computer opening. After it finishes, click lower-right cup (360,220): stones are sown, one stone enters the player's right scoring cup and the computer replies. No-button control retains its settled board and empty right store. All four active/control repeats on merged34787a02 and background candidate722b0c8e pass13 checks: eight measured causal pixels and five exact own-run captures, frontend1150/ guest1726 ticks, zero exhausted frames. Actual captures viewed; initial images match, and the settled no-input board remains unchanged. Own-run consistency is not hardware accuracy. Merged-runtime binary SHA-256 a2d146414749fd6faf9f89e5546420077c4e3423ffadaf2928d0e6cfa838d0bc. An earlier click during the opening was ineffective and is retained as diagnostic evidence. Browser input/pacing, full games, save, audio fidelity and released public play remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5815
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: cfb5dd292538cc7352534650c88bd86b58de3f0c private browser on d7de5122 runtime
    architecture: 68k
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5815
    environment: >-
      Fresh Chromium 156/Linux aarch64 profiles, isolated worker/WebGL and
      SharedArrayBuffer; original 68K/default 25 MHz/800x600/depth8 and original
      Medium difficulty/options. Each unchanged 20-second sample includes
      Game Play Game and any computer opening, followed by an active lower-right
      cup click or matched no-button pointer control. Active sowing earns a point
      in the player's right store and the computer replies; control retains an
      empty player store. Actual before/final captures inspected. Independent
      coin-toss openings; no identical-opening or general determinism claim.
      Active/control steady guest 60.9372/60.8760 ticks per second and host
      59.9216/59.8614 FPS; maximum worker frames 29.9/25.9 ms, minimum audio
      queues 121.7/127.0 ms. All unchanged pacing gates pass; no captured
      lifecycle/network errors. These are steady snapshots, not whole-game
      averages. Actual source stamp, game ID, worker, 68K, 25 MHz, isolation and
      shared memory checked. Original archive and both main/worker Wasm HTTP 200
      response bodies match independent integrity pins. Wasm 17,921,089 bytes,
      SHA-256 8a49343a8742faea98f70dc8a4f6eeda13283cf7a23fdc370bded79b5531dc84.
      Runtime d7de5122 has the same full source tree as native candidate722b0c8e.
      Full games, save, audio fidelity and released public play remain unverified.
      Final runtime/entry CI and integration remain pending.
runtime:
  executable_path: YA-Mancala v1.0.1ƒ folder/YA-Mancala v1.0.1ƒ/YA-Mancala v1.0.1
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 3d718f78861de6d58c176960fc2e30862a9077ed4f9f0dfb9011f0351a1875f8
    size_bytes: 106332
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/brd/ya-mancala-101.hqx
    - https://github.com/benletchford/systemless/issues/5815
    license: Original unchanged noncommercial distribution terms
    rights_holder: Keith Rose
    permission: >-
      Complete unique 18547-byte README explicitly permits unchanged noncommercial
      distribution with the document and prohibits profit from distribution. Retain the
      original 30-day evaluation and Can$10 shareware request; copies retained solely
      for distribution have an explicit expiry exemption. No date, registration or game
      patch.
    notes: >-
      Both duplicate original folders retained. Independent production-pack parity
      passes four files, eight forks and four Finder records; strict KPK304158 bytes,
      SHA-256 7e5bc56fdcdd9c14d30ad9a40b37a69c32a8ff9d1c5c1ea4417d570b2b29b03b. Original
      68K-only application; year1995 follows original October1995 app timestamps. No
      external runtime supplied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: d03b57a95cac05f9af82d82c7fc329f366d9a65d499308b50f1bab3693457a17
    size_bytes: 26235
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5815
    rights_holder: Keith Rose
    permission: Fresh content-only capture from the unchanged distributable original.
    notes: >-
      Exact crop (4,40)-(504,342), no resampling; legal sowing, player point and
      computer reply.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/brd/ya-mancala-101.hqx
- https://github.com/benletchford/systemless/issues/5815
---

![Mancala after sowing and a computer reply](https://assets.systemless.org/catalogue/media/sha256/d0/d03b57a95cac05f9af82d82c7fc329f366d9a65d499308b50f1bab3693457a17.png)

Choose Play Game from the Game menu and let the computer finish any opening move.
Click a cup on your side to sow its stones counterclockwise. Finish in your scoring
cup for another turn; finish in an empty cup on your side to capture stones from
opposite. The original README explains the rules and options. This is the original
Can$10 shareware release with a 30-day evaluation period.
