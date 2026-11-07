---
id: super-othello
kind: game
title: Super Othello
summary: Trap and flip opposing pieces on an 8x8 board in Yellow Lemon Software’s 3D-styled Reversi game.
developer: Daniel Stødle
publisher: Yellow Lemon Software
year: 1997
architectures:
- 68k
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: playable
  verified:
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: fc327b1bd49f148a45356a37b1d2f25aac4f5c96 native; runtime unchanged through b4f88657
      master
    architecture: ppc
    environment: Complete unchanged original; exact executable; default PPC/25 MHz/800x600/16-bit colour,
      no depth override. Ordinary splash click, Not yet, and little-board New Game reach green’s opening
      turn. Legal click (330,221) places green, flips the intervening red piece and changes counts 2–2
      to 4–1, then indicates red’s turn. Equal-clock omitted-click control retains opening board/counts
      2–2. Fresh repeats each pass eight causal pixels and five exact prior captures at frontend/guest
      1216, zero exhausted frames. Actual captures inspected; pre-input images match. Own-run consistency
      is not hardware accuracy. Binary SHA-256 ec1e070bb9deb63e84a18d0f897c28cd7197428bb3d300f84a36b98ed2c44a4f.
      A transient splash overlaps the registration reminder before ordinary dismissal; subsequent board
      is clear. Full games, AI, save/log, tutorial, audio fidelity, 68K gameplay and browser/public launch
      remain unverified. The same 13-check active/control replays also pass on MapRgn candidate 9dde754438eddce4d35f0bfc0a02a1d92b088483,
      binary SHA-256 b2aada24f38cf1f555f6b76939bfee827d3a1656e3c6915ca1af92e8a408b8ff, matching all prior
      images exactly.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5799
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: fafe2deb5681cd3c1b32e705ca360cd7af87acdc private browser on 9dde7544 runtime
    architecture: ppc
    environment: Fresh Chromium isolated worker/WebGL profiles and SharedArrayBuffer, exact original executable,
      default PPC/25 MHz/800x600/16-bit colour; no game-setting or depth override. Ordinary audio gesture,
      splash, Not yet and little-board New Game precede each unchanged 20-second gameplay sample. Inspected
      original 2–2 board; legal F4 click places green, flips red and ends 4–1/red turn. No-button pointer
      control retains 2–2/green turn. Actual opening/final captures inspected. Active/control steady guest
      59.9292/60.7005 and host 59.9292/59.6888 per second; maximum worker frames 37.6/33.1 ms, minimum
      audio queues 113.9/118.7 ms. All unchanged gates pass. Game ID, worker, PPC, 25 MHz, full private
      source stamp, isolation and shared memory checked. Actual original archive and both main/worker
      Wasm HTTP 200 response bodies match independent byte/hash pins. Wasm 17,885,492 bytes, SHA-256 4afd18decea8571157c5289e814149ae34f2b640f2c3e35c6d46aed50168f55e.
      No captured lifecycle/network errors. Rates are steady snapshots, not whole-game averages. AI, complete
      games, save/log, tutorial, audio fidelity, 68K gameplay and released public play remain unverified.
      Runtime/browser implementation content matches merged b912638f7bdd236bb2a03d9aa67e340981e427b0.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5799
runtime:
  executable_path: Super Othello 2.3 (FAT)
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: b4d37ff7f6533b934ede476f3e8f081bfb6b58f1022100bcf0c5f25e5b5a711f
    size_bytes: 998554
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/brd/super-othello-23.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/brd/00brd-abstracts.txt
    - https://github.com/benletchford/systemless/issues/5799
    license: Yellow Lemon Software nonprofit unchanged distribution licence
    rights_holder: Yellow Lemon Software / Daniel Stødle
    permission: The complete included License Agreement grants nonprofit distribution without prior notice
      when software, documentation and licence are included unchanged. Preserve the entire original package,
      $15 shareware notice and 30-day trial. Screenshots are expressly allowed; no graphics reused separately.
      All seven plaintext documents reviewed in full. No registration, date, settings, application or
      external-runtime modification; no instruction disassembly.
    notes: 'Independent extraction matches production packing: nine paths, eighteen forks, nine Finder
      records; strict KPK 1,611,553 bytes, SHA-256 9daeece5befa54882039933a81fd7cf911f616e054b9c00e911e67e3db4abf28.
      Both original CPU slices retained; default PPC qualified natively, 68K unqualified.'
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f9ed28781ada7b5d380fcb571149666f60c69f55eb119c7e5444ccb93521fc87
    size_bytes: 190361
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5799
    rights_holder: Yellow Lemon Software / Daniel Stødle
    permission: Fresh screenshot of the unchanged original; screenshots expressly permitted by the included
      licence.
    notes: Exact content crop (56,46)-(577,467), 521x421, no resampling; ordinary green move changes counts
      to 4–1. Excludes menu and title bar; retains complete game content.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/brd/super-othello-23.hqx
- https://github.com/benletchford/systemless/issues/5799
launch_enabled: true
---

Dismiss the original shareware reminder with Not yet, then click the little board at the right to start a game. Green moves first. Click an empty square that traps opposing pieces between the new piece and another of your colour; the trapped pieces flip. The Othello menu offers opponent and intelligence settings. The original $15 shareware notice and 30-day trial remain in the package.
