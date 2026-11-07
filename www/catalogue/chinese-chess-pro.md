---
id: chinese-chess-pro
kind: game
title: Chinese Chess Pro
summary: Play Chinese chess against the Macintosh in Tie Zeng's original shareware game.
developer: Tie Zeng
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged 1.0.1 package, exact Color selector, explicit 68K and
      800x600/depth 8. Ordinary drag moves the central red soldier one rank forward from
      (159,265) to (159,231). The computer answers with a blue horse move from (57,61) to
      (91,129). Equal-clock omitted-drag control keeps both starting positions. Fresh
      active/control repeats each pass eight measured pixels plus two exact own-run
      captures, ten checks at 444 frontend/1044 guest ticks, zero exhaustion. Own-run
      consistency is not hardware-oracle accuracy. Runtime Rust/Cargo source matches merged
      8dbc14c6; native binary SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Independent production packing matches seven paths, fourteen
      forks and seven Finder records. All three executables have CODE 0-22 and no PPC
      slice. Opening capture has a left-edge redraw artifact; the inspected post-move board
      redraws the pieces. Startup fidelity, browser input/pacing, full games, stronger
      levels, persistence and sound fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2318
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 39fb36d4304ee1d99332d62f6ae650ead1228ef8 private browser candidate
    architecture: 68k
    environment: >-
      Optimized private HTTPS frontend on merged runtime 8dbc14c6,
      Chromium 156.0.8078.79/Linux aarch64, isolated worker/WebGL, 25 MHz,
      800x600/depth 8, deployed COEP credentialless/COOP same-origin and
      SharedArrayBuffer verified. Actual hosted HTTP 200 archive response matches
      242056 bytes and its SHA-256. Main and worker Wasm responses match
      17769535 bytes, SHA-256 734413441ec31cb800f0091c67902960c06b4ccff9eb20414bc96b3f70316d2a.
      Trusted input attempts two red soldier moves and receives computer
      horse/rook movement, but the second capture does not visibly retain the
      previously advanced central soldier where expected. Rendering needs
      diagnosis. The unchanged 20-second gate fails at final steady guest
      35.5324 ticks/sec, host 55.1688 replies/sec; maximum frame 45.2 ms,
      minimum audio queue 115.0 ms, zero captured lifecycle/network errors.
      Final snapshots are not sample-wide means. No matched browser control
      or launch approval. Source stays disabled; full games, startup fidelity,
      stronger levels, persistence, audio and public play remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2318
runtime:
  executable_path: Chinese Chess Pro 1.0.1/Chinese Chess Pro Color
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: a9b0f0f8686d9f104fef15fa3d1994cdf5cec502e6ed02d5da902d35a769fb9f
    size_bytes: 242056
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Chinese%20Chess%20Pro%201.0.1.sit
    - https://github.com/benletchford/systemless/issues/2318
    license: Original complete-package distribution grant
    rights_holder: Tie Zeng and credited contributors
    permission: >-
      Retained About documentation expressly permits unchanged complete-package
      distribution to individuals and user groups, and upload to commercial or non-profit
      bulletin boards. Inclusion in software packages sold for profit requires the
      author's written permission. Preserve every original file, opening book and notice,
      including the shareware registration and thirty-day conditions, author credit and
      Kui Yu Chang's icon credit. No paid gate or registration behaviour is bypassed.
    notes: >-
      Original archive is 242056 bytes, SHA-256
      a9b0f0f8686d9f104fef15fa3d1994cdf5cec502e6ed02d5da902d35a769fb9f. All retained data/resource notices reviewed; no
      exclusive-media clause found. Original Color resource fork is 191091 bytes, SHA-256
      510d90715bf6940df7e4cceee84bee54ffb4e3326928e6553b157c728e312f7b. The archive is
      labelled 1.0.1; the original About labels Color/Sr as 1.0. Preserve these original
      variant identities. They comprise one distinct game. Original About copyright
      identifies 1993.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 3313686b26e48ace520fa7d05ee4914904849fcba229391ea4c568843eb14ef3
    size_bytes: 7481
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2318
    permission: Original gameplay capture from the permitted unchanged package.
    notes: >-
      Exact 308x341 board crop at (5,44), after a legal red soldier move and blue
      horse reply. No resampling, menu bar, window framing, chat fields or host interface.
references:
- https://github.com/benletchford/systemless/issues/2318
---

![Chinese Chess Pro after a red soldier move and blue horse reply](https://assets.systemless.org/catalogue/media/sha256/33/3313686b26e48ace520fa7d05ee4914904849fcba229391ea4c568843eb14ef3.png)

Drag a red piece to a legal destination to play against the Macintosh.
A soldier moves one rank forward before crossing the river. **Game → Configure
Game** chooses sides and timing; **Level** adjusts the computer's strength.
**File → New** starts again. The board's history slider steps through moves.

This is the complete original shareware package with its registration terms.
Native play verifies one soldier move and the computer reply. Browser pacing
fails and a later redraw needs diagnosis, so launch remains disabled. Startup
rendering fidelity, full games, stronger levels, persistence, sound fidelity and
released public play remain unverified.
