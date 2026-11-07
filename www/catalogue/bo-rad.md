---
id: bo-rad
kind: game
title: Bo-Rad! 1.01
summary: Crack a hidden five-color code using black and white clues.
developer: Joseph Fleck
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: f9712bfca41d87823175e38def2f5eeb06f85db1 native release-base candidate
    architecture: 68k
    environment: >-
      Complete unchanged original, exact executable, explicit 68K/default 25
      MHz/800x600/depth 8. Ordinary Command-N opens a game. Palette clicks and keys 1–5 fill a
      complete red/white/green/blue/yellow guess. Return submits it and produces
      black/white feedback markers. Equal-clock control retains the same filled guess and
      omits only Return, leaving feedback unsubmitted. Before-submit images match exactly;
      final differences are confined to the feedback region. Fresh active/control
      repeats each pass four measured pixel assertions and five exact earlier own-run
      captures, nine checks at 492 frontend/1092 guest ticks with zero exhaustion. Own-run
      references establish consistency, not hardware-oracle accuracy. Native SHA-256
      1e927387546f854ae55878cef45f5580cb381dc11831892536f2d01b0d855a0d; runtime src equals
      master 3c52cb37. Original hidden puzzle, Repeat Colors/Mix Em Up settings and
      shareware request remain intact. Complete rounds, clue-rule accuracy across all
      combinations, mixed-puzzle mode, saves/high-score writes, sound fidelity, browser
      pacing and released public play remain unverified. Launch stays disabled pending
      browser approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5754
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 9ba0c5e9ca7754fa0adfc0054befe70b51ed75b4 private browser candidate
    architecture: 68k
    environment: >-
      Clean private HTTPS frontend on merged runtime 3c52cb37, Chromium
      156.0.8078.79/Linux aarch64, worker/WebGL/SAB, default 25 MHz and 800x600/depth 8.
      Production isolation headers and actual private build stamp, architecture and clock
      checked. Original hosted archive HTTP 200 matches its catalogue size/hash. Main and
      worker Wasm HTTP 200 responses match the independently hashed build: 17847979 bytes,
      SHA-256 e575dc146678e254485d2131fb8b862ff7168dd7de1f3e80cd8af78ef7e36ac9. Ordinary
      Command-N, palette clicks and keys 1–5 enter a complete guess. Return adds feedback
      and advances input to the second row, verified by entering its first green piece;
      omitting Return retains grey feedback and subsequent input edits the first row.
      Separate unchanged 20-second samples preserve the original clock, hidden puzzle and
      game settings. Final steady guest/host snapshots are 34.1615/61.0766 and
      36.6140/60.0063 per second. Both guest rates fail the unchanged minimum of 50
      ticks/sec. Maximum worker frames 22.5/22.0 ms and minimum audio queues 132.8/132.4
      ms pass their limits; zero captured lifecycle/network errors. Snapshots are not
      full-round averages. Browser input works but pacing is not approved; overall status
      stays unknown and launch remains disabled. No lighter clock, easier preset or
      relaxed gate is used. Full rounds, mixed-puzzle mode, persistence, sound fidelity
      and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5754
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: d9255200a9a892c3de8002fd975dc02802fdbae9 native polling candidate
    architecture: 68k
    environment: >-
      Fresh optimized build, complete unchanged original, exact selector, default 25
      MHz/800x600/depth 8 and original puzzle preferences. Ordinary complete guess and
      Return submission versus matched no-Return control each pass four measured pixel
      assertions and five exact pre-change captures, nine checks at 492 frontend/1092
      guest ticks with zero exhaustion. Both final images inspected. Control trap
      dispatches fall from 11098671 to 20321 through the proven idle-cycle improvement;
      native wall time is diagnostic, not browser pacing evidence. Binary SHA-256
      09d53dffaf8730437e184ea004f80d4b8100d184263665ff43c7143c01916d17.
      Full rounds, mixed-puzzle mode, persistence and sound fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5758
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1b47822e0b25cd4b9457bd5a6aecbfbd5f188c2c private browser polling candidate
    architecture: 68k
    environment: >-
      Fresh private HTTPS frontend with runtime/build code equal to d9255200,
      Chromium 156.0.8078.79/Linux aarch64, worker/WebGL/SAB, default 25 MHz and
      800x600/depth 8. Actual build stamp, architecture, clock and production isolation
      checked. Hosted original archive HTTP 200 matches its catalogue size/hash; actual
      main and worker Wasm responses match the independently hashed 17848166-byte
      build, SHA-256 330cc38e3579a7b3a4bcf76d549792d91ef0549fd7aa5f311fbb3a65f5c1362c.
      Ordinary Command-N, palette clicks and keys 1–5 fill a complete guess. Return
      submits it with feedback and subsequent input fills the second row; matched
      no-Return control retains grey feedback and edits the first row. Visible state
      and final images checked. Separate unchanged 20-second samples preserve original
      hidden puzzle and settings. Active/control steady guest rates are 60.3534/60.0432
      ticks/sec and host rates 59.3475/59.0425 FPS. Maximum worker frames 41.0/42.1 ms
      and minimum audio queues 113.7/109.0 ms pass all unchanged gates (guest at least
      50/sec, host at least 55 FPS, worker at most 50 ms, audio at least 40 ms).
      Zero captured lifecycle/network errors. Steady snapshots are not full-round
      averages. Browser launch approved after resolving the earlier pacing failure.
      Full rounds, clue-rule accuracy across all combinations, mixed-puzzle mode,
      persistence, sound fidelity, hardware-oracle accuracy and actual released public
      play remain unverified; public replay requires a human release.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5754
runtime:
  executable_path: BinHex File/Bo-Rad! v1.01 ƒ/Bo-Rad! v1.01
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 6d8d5f50081b78fd2759cfa4ed792b170c403627d8df212319654f34ba992cd6
    size_bytes: 170286
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/bo-rad-101.hqx
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/00brd-abstracts.txt
    - https://github.com/benletchford/systemless/issues/5754
    license: Original author permits copying the complete unchanged package
    rights_holder: Joseph Fleck
    permission: >-
      The original Info-Mac submission quotes Joe Fleck allowing the game to be
      copied and passed along while preserving the original files, with an exception for the
      high-score list. Retain every file unchanged, including original highscores,
      both documentation files, copyright and the $7 shareware request. About and
      version-history documentation identify Joseph Fleck and the 1.01 update.
    notes: >-
      Independent unar/lsar extraction matches all four original inner paths, eight
      forks and four Finder records; strict KPK end 263270, pack SHA-256
      4caef0fe446d78c17efa85888b540b92f3bcdd6f3c823565842f58398df10519. Independent no-recursion
      BinHex decoding names the CompactPro wrapper Bo-Rad! v1.01 ƒ.cpt. Production uses the
      synthetic BinHex File/ outer namespace for this MacRoman wrapper name; only that
      virtual prefix is mapped for comparison. Original inner names, forks, Finder
      metadata and the complete published BinHex bytes are unchanged. App is resource-only,
      CODE 0/1/3/4 and no cfrg/native PPC slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 769fb40e225c19c1a975aae76c45e13de5d3e98fc1c71310040d745949bde92c
    size_bytes: 7046
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5754
    rights_holder: Joseph Fleck
    permission: Fresh gameplay capture from the permitted unchanged game distribution.
    notes: >-
      Exact 508x317 native content crop at (146,151), excluding the Classic menu bar
      and window frame; no resampling. Shows a submitted guess and feedback without
      revealing the hidden puzzle. Not an independent hardware reference.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/00brd-abstracts.txt
- https://github.com/benletchford/systemless/issues/5754
---

![Bo-Rad after submitting a guess](https://assets.systemless.org/catalogue/media/sha256/76/769fb40e225c19c1a975aae76c45e13de5d3e98fc1c71310040d745949bde92c.png)

Choose **File > New**. Click a palette color and press **1–5** to fill that
column, then press **Return** or **Check It!** to submit the complete row.
A black clue means the correct color in the correct position; a white clue
means the correct color in another position. Clue order does not identify
which guess position it describes.
