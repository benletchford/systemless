---
id: macgnugo
kind: game
title: MacGnuGo
summary: Play Go on a full-size board against the GNU Go 2.6 engine.
developer: Jeremy Harris / GNU Go contributors
year: 2001
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: c796d054 native; runtime code identical to merged 3fe927b1
    architecture: 68k
    environment: >-
      Complete unchanged GnuGo MacPack and exact GnuGo MacPack/MacGnuGo executable,
      explicit 68K, default 25 MHz/800x600/depth 8, without a partition override.
      Ordinary File/New Game selects GnuGo for White, retaining Black Human, the 19x19 board,
      komi 5.5, handicap zero and original strength. A legal black D4 click is
      followed by the white computer reply at Q15. Equal-clock omitted-button control moves
      the same pointer and retains the empty board. Each fresh repeat passes eight
      measured causal pixels and six exact prior captures, fourteen checks at 1070
      frontend/1670 guest ticks with zero exhaustion. Actual images inspected; own-run references
      establish consistency, not hardware accuracy. Native binary SHA-256
      7ca0f7195dc8910fe687bf0d470cf45a7e2de94a60a528a964fc5b7600867e03. The original author
      documents possible ko/suicide errors and late-game assertions; the game is not patched.
      Complete games, captures/ko/scoring, persistence and audio fidelity remain
      unverified. Hosted-original browser evidence follows below; released public play
      remains unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5829
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1539c1a0 private browser preview on merged 3fe927b1 runtime
    architecture: 68k
    environment: >-
      Fresh isolated Chromium/Linux profiles, worker/WebGL and SharedArrayBuffer,
      default 25 MHz/800x600/depth 8, production COEP credentialless and COOP same-origin
      headers. Source stamp, worker, architecture and runtime settings checked. Each
      unchanged 20-second sample includes New Game, White GnuGo/Black Human selection,
      the original 19x19 board, komi 5.5, handicap zero and unchanged strength. Active
      black D4 receives a visible white computer reply in the upper right; the matched
      omitted-click control retains an empty board. Actual dialogs and board captures
      inspected. No hidden warm-up gameplay, seed changes or game modifications.
      Final steady guest/host rates are 59.9959/59.9959 and 60.0023/60.0023 per second;
      maximum sampled worker frames 20.7/16.1 ms and minimum audio queues 120.3/132.9 ms.
      Both pass the unchanged pacing gates with no captured lifecycle or network errors.
      These snapshots are not whole-game averages. Original archive HTTP 200 body is
      1417743 bytes, SHA-256 650bfddebb912ff746a3ce63f2fab33f962c858ced3daa0bfab1725b06c293af.
      Both main and worker Wasm HTTP 200 bodies match the build, 17933867 bytes,
      SHA-256 5e9177954fd1fec2abf06f9fb48d631afeedfa2d2495cb37fd50547a6ff7dcfc.
      Matching GNU Go 2.6 source supplement independently verified HTTP 200, 617161 bytes,
      SHA-256 75dbedf7f379d3e3d7ae2abc8a19e03f4a8ca632282817e298e7465e2e0d3c7d.
      Runtime code remains identical through ddc57acf; only Domain/Fractal catalogue
      metadata changed. This establishes bounded opening play and browser pacing.
      Original ko/suicide and late-game assertion bugs remain unchanged. Complete games,
      captures/ko/scoring, persistence, audio fidelity and released public play remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5829
runtime:
  executable_path: GnuGo MacPack/MacGnuGo
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 650bfddebb912ff746a3ce63f2fab33f962c858ced3daa0bfab1725b06c293af
    size_bytes: 1417743
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/brd/gnu-go-05.hqx
    - https://github.com/benletchford/systemless/issues/5829
    license: GNU GPL version 2 and original Mac source-retention notice
    rights_holder: Jeremy Harris; GNU Go authors; retained third-party notices
    permission: >-
      The complete README_code explicitly permits distribution anywhere provided the
      code and message accompany the application. The original full COPYING supplies
      GPLv2. Preserve both original frontends, all MacCode sources, patches, resources
      and notices. The matching upstream GNU Go 2.6 source required by README_code is
      supplied as a managed supplement alongside it.
    notes: >-
      Original vers 128 identifies version 0.5, GNU Go engine 2.6, June 2001. Full
      7392-byte README, 5318-byte README_code and 17977-byte COPYING reviewed. Source and
      resource notices are retained. Independent extraction matches all 37 paths, 74
      forks and 37 Finder records, including resource-only apps and source resources.
      Strict KPK end 3413339, SHA-256
      ec80ad1378b63564fcc5dbdf3c909e59de3d322c635e91102e2cbfec9462c4f8. Both original Go frontends remain in the archive; only the
      MacGnuGo GUI is verified, and this package is one game intake rather than two game
      counts.
- id: engine-source
  role: supplement
  format: gz
  source:
    type: sha256
    sha256: 75dbedf7f379d3e3d7ae2abc8a19e03f4a8ca632282817e298e7465e2e0d3c7d
    size_bytes: 617161
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.gnu.org/gnu/gnugo/gnugo-2.6.tar.gz
    - https://github.com/benletchford/systemless/issues/5829
    license: GNU GPL version 2
    rights_holder: GNU Go authors / Free Software Foundation
    permission: >-
      Complete unchanged GNU Go 2.6 base source, accompanying the original Mac source
      files, patches and resources retained in the game archive. Preserve the original
      GNU COPYING and notices; supply this source download alongside the binary under
      GPLv2. No source is compiled or game binary modified.
    notes: >-
      Official GNU HTTPS download matches the integrity pin. Gzip/tar parses with 148
      members under gnugo-2.6; configure metadata identifies 2.6 and required
      engine/interface source files are present. README_code specifically names 2.6 and warns
      against substituting 2.7.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b6e6ae245ff8ca6cb22e8a99fd252322418557422b476cf5457e8509fe75c0d5
    size_bytes: 1910
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5829
    license: GNU GPL version 2
    rights_holder: Original game contributors
    permission: Original gameplay capture of the unchanged GPL-licensed game.
    notes: >-
      Original black D4 move and white Q15 reply. Native 800x600 capture cropped to
      (21,63)-(382,424), 361x361, without resampling; excludes menu bar, native window
      framing and host interface.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/brd/gnu-go-05.hqx
- https://ftp.gnu.org/gnu/gnugo/gnugo-2.6.tar.gz
- https://github.com/benletchford/systemless/issues/5829
---

## Surround territory

![Black D4 and the computer's white Q15 reply in MacGnuGo](https://assets.systemless.org/catalogue/media/sha256/b6/b6e6ae245ff8ca6cb22e8a99fd252322418557422b476cf5457e8509fe75c0d5.png)

Choose File → New Game. Select GnuGo for White to play against the computer,
keep Black as Human, and press OK. Click an empty intersection to place a stone.
The original README explains saving games and the available analysis tools.

This early release has known game-rule and late-game bugs documented by its
original author. The complete original package and corresponding source are
retained.

[Download the matching GNU Go 2.6 source](https://assets.systemless.org/catalogue/objects/sha256/75/75dbedf7f379d3e3d7ae2abc8a19e03f4a8ca632282817e298e7465e2e0d3c7d.gz).
The Mac-specific source files and patches accompany the original game download.
