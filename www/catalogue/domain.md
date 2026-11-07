---
id: domain
kind: game
title: Domain
summary: Clone or jump pieces across the board, capturing adjacent opponents to claim the larger
  domain.
developer: Mike Perry
year: 1998
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: 'Complete unchanged original, exact executable, 68K/default 25 MHz/800x600/depth
      8. Dismiss title, New Game, acknowledge computer-first dialog. Select red (212,150), adjacent
      (266,150): red clones and computer replies. Equal-clock omitted-destination control keeps
      original position. Repeats each pass eight causal pixels plus two exact prior captures
      at frontend 860/guest 1460, zero exhausted frames. Actual captures inspected; own-run
      repeatability is not hardware accuracy. Known limitation: counters remain User 2/Computer
      3 while board has 3 red/4 green. Guest DrawString requests 3/4 but retained board clipping
      excludes counters; cause remains unproven. No compensation or game alteration. Binary
      SHA-256 35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34. Full games,
      final scoring, alternate boards/difficulty, audio fidelity and browser pacing remain unverified.
      Launch disabled pending browser qualification.'
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5790
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: ppc
    environment: Original fat PowerPC slice, default depth. Halts at unsupported InterfaceLib:InitPack
      before completing the first frontend tick. No gameplay. Use 68K.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/5790
  - date: '2026-11-08'
    tester: Catalogue maintainer
    systemless_version: d822e8ae4c3f3e36f7391d3a60ae22600d8dbe43 private browser on 2fe0e5e5
      runtime
    architecture: 68k
    environment: Fresh Chromium isolated worker/WebGL profiles, SharedArrayBuffer; exact executable;
      68K/default 25 MHz/800x600/depth 8. Ordinary title dismissal, File > New Game and first-turn
      acknowledgment precede each unchanged 20-second sample. Both select red source (212,150);
      only active clicks adjacent (266,150). Active clones and computer replies; control retains
      selected red source. Actual captures inspected. Browser active counts correctly show 3/4,
      unlike stale native captures; a longer native-click probe still shows stale counters,
      so input duration alone does not explain the difference. Active/control guest rates 60.0063/59.0920,
      host rates 60.0063/60.1108 per second. Maximum worker frames 51.7/35.5 ms; minimum audio
      queues 108.3/115.9 ms. Active FAILS unchanged 50 ms worker-frame gate at 51.7 ms, 5.3
      seconds into sample during computer reply; control passes all gates. No blind rerun or
      threshold change. Original archive and main/worker Wasm HTTP 200 response bodies match
      byte/hash pins. Game ID, worker, architecture, 25 MHz, private source stamp, isolation
      and shared memory checked. Wasm 17879871 bytes, SHA-256 f7020e8e163c23a9678506ea7c99d23c22f7f16df65571649cbb129b1d7f35b1.
      No lifecycle/network errors. Overall unknown and launch disabled pending pacing resolution.
      Full games, final scoring, alternate boards/difficulty, audio fidelity and released public
      play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5790
runtime:
  executable_path: Domain 1.0/Domain (FAT)
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: a9ffd119dfe354b271396c8c34e438ba3a2d07e20e24375216810a2431b744bc
    size_bytes: 457321
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/domain-10.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/00game-abstracts.txt
    - https://github.com/benletchford/systemless/issues/5790
    license: Original unchanged distribution terms
    rights_holder: Mike Perry
    permission: Complete original HTML instructions explicitly permit copying in original form
      with all instructions, free of charge. Postcardware; preserve the whole original archive,
      HTML instructions, instructional JPEGs, forks and notices. Full 2563-byte instructions
      read.
    notes: 'Independent extraction matches production packing: five paths, ten forks, five Finder
      records, strict KPK end 744892, pack SHA-256 0234febf825b6d0995da1aefe59ee146df9840804098c5e5064a8fa2bfc02512.
      Original StuffIt-in-BinHex, fat 68K/PowerPC executable; original copyright identifies
      1998. Only 68K gameplay qualified; PPC halts at InitPack.'
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f8fb5347bfd97d13e483c221e3c422305e5151b78135d8af6b415993de606cbf
    size_bytes: 35680
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5790
    rights_holder: Mike Perry
    permission: Fresh content-only capture of the unchanged distributable original.
    notes: Exact crop (82,121)-(718,559), 636x438, excluding window chrome, no resampling. Shows
      ordinary clone and reply, including the known stale side counters.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/00game-abstracts.txt
- https://github.com/benletchford/systemless/issues/5790
---

Click to dismiss the title, then choose New Game from the File menu. Select one of your red pieces, then a destination: an adjacent square clones the piece; a square two spaces away jumps it. Enemy pieces beside the destination become yours.

**Qualification hold:** the active browser run exceeds the unchanged worker-frame limit (51.7 ms versus 50 ms), so browser launch remains disabled.

**Known limitation:** native side counters can stay stale after moves; the measured browser run showed correct counts. The board shows the changed pieces; final scoring has not been verified. Use the 68K version; the PowerPC version currently stops during startup.
