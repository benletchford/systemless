---
id: pair-picker
kind: game
title: Pair Picker
summary: Remember card faces and uncover matching pairs in solitaire or two-player Concentration.
developer: Al Spohn
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: Complete unchanged original; exact executable; 68K/default 25 MHz/800x600/depth
      8. Ordinary New Game, first card reveals an American flag. Second non-matching selection
      hides both cards and increases miss count 0 to 1; equal-clock omitted-second-click control
      retains flag/count 0. Repeats each pass eight causal pixels and two exact prior captures,
      frontend 492/guest 1092, zero exhausted frames. Actual captures inspected. Own-run consistency
      is not hardware accuracy. Binary SHA-256 35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34.
      Additional ordinary matching-flag pair removal passes four causal pixels and two exact
      captures, same clocks and zero exhausted frames. Full games, two-player mode, bonus cards,
      high-score persistence and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5788
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: ba962a2916bf77316e7ea4cdfd51f30802dcd734 private browser on 0dbf0560
      runtime
    architecture: 68k
    environment: Fresh Chromium isolated worker/WebGL profiles, SharedArrayBuffer; exact executable,
      68K/default 25 MHz/800x600/depth 8. Ordinary blank-canvas audio gesture precedes each
      unchanged 20-second sample. Both select the first card; active selects a non-matching
      second card, hides both and increases miss count 0 to 1. Control omits second click and
      retains first face/count 0. Independently shuffled card faces differ (rocket versus bottle);
      no same-deck browser determinism claim. Actual captures inspected. Active/control steady
      guest and host rates 60.0054/59.9972 per second; maximum worker frames 37.2/20.3 ms, minimum
      audio queues 116.2/132.3 ms. All unchanged pacing gates pass. Game ID, worker, architecture,
      25 MHz, private source stamp, isolation and shared memory checked. Actual original archive
      and main/worker Wasm HTTP 200 response bodies match independent byte/hash pins. Wasm 17876151
      bytes, SHA-256 feaf8bfcf3346830ee590d8bb87f7824b9c2fb3cec61de9fca8528404c11fa9b. No lifecycle/network
      errors. Rates are steady snapshots, not whole-game averages. Full games, two-player mode,
      bonus cards, high-score persistence, audio fidelity and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5788
runtime:
  executable_path: PairPicker1.0.sea/Pair Picker 1.0 ƒ/Pair Picker 1.0
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 10bb0b28386c582e6748506c9de50654df52356cea531a53fde440f3e54f12ba
    size_bytes: 117288
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/crd/pair-picker-10.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/crd/00crd-abstracts.txt
    - https://github.com/benletchford/systemless/issues/5788
    license: Original unchanged non-profit distribution terms
    rights_holder: Al Spohn
    permission: The complete original documentation explicitly encourages copying and distribution
      with its notice preserved; sale or distribution for profit requires permission. Preserve
      the entire original archive, compiled documentation and notices. Full manual TEXT 128
      reviewed. Original shareware asks $10, with no play-count or time limit.
    notes: 'Independent extraction matches production packing: two paths, four forks, two Finder
      records, strict KPK end 237396, pack SHA-256 2fd50daa07bcb1702902cb8889cae484e194c4dcbebdf7e7a9062df643a14ca2.
      Compact Pro-in-BinHex, original 68K application; no external runtime supplied.'
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 05b6a6832d7fc5898622e84e7944e803f275b4fd0bc9afaa0073d1eb2c892ba4
    size_bytes: 5177
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5788
    rights_holder: Al Spohn
    permission: Fresh content-only capture of the unchanged distributable original.
    notes: Exact crop (148,169)-(650,468), 502x299, no resampling; card face revealed during
      ordinary play, excluding window chrome.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/crd/00crd-abstracts.txt
- https://github.com/benletchford/systemless/issues/5788
launch_enabled: true
---

Click a card to reveal its face, then choose another. Matching pairs disappear; in solitaire, each missed pair increases the counter. Choose Solitaire or 2 Players from the Game menu. New Game is in the File menu.
