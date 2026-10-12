---
id: bubbles
kind: game
title: Bubbles
summary: Drag matching balls into five-ball lines before the board fills.
developer: Maxym Runov
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: fb3e11d3b67895a21e3dae7b05ca7fba4615fb12
    architecture: ppc
    environment: >-
      Fresh optimized native build, untouched original 1.3 archive and exact PPC
      executable selector below, depth 16. Ordinary File, New Game and OK render the Small
      board. Original masked translucent drag image, ball drops, five-ball removal and
      score 50 pass thirty measured pixel assertions at 2050 frontend / 2021 guest
      ticks, zero exhausted frames. Captures inspected. Thirty-seven drag-related tests
      pass. A separate nine-assertion replay and cell/drawing traces confirm source
      clearing followed by new-ball repopulation. Six inner VISE files/twelve forks and
      type/creator records match an independently implemented pinned ScummVM-reference
      reader. Original Finder flags are not recovered by that reference; original archive
      bytes remain unchanged. Full games, saves and audio fidelity remain unverified. The
      later verified source below fixes browser drag feedback and wait-clock pacing.
      Subsequent verification below completes asset promotion and launch approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4692
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: f5483633a23f28ddf6bd2e55f8f712ce04a69b9d
    architecture: ppc
    environment: >-
      Fresh optimized native build from merged master with m68k 0.14.4 / PPC 0.7.2,
      configured depth 8, untouched original archive and exact selector. Continue at the
      original colour advisory, then normal New Game/OK, held translucent image, ball
      drops, five-ball removal and score 50 pass thirty measured pixel assertions at
      2176 frontend / 2148 guest ticks with zero exhausted frames. Captures inspected.
      The catalogue screenshot matches the fresh gameplay content crop byte for byte.
      Full-game, save and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4722
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 1fbc1a9b7c078562231459faf50c1ff0122d037f
    architecture: ppc
    environment: >-
      Local Chrome Worker replay of the unchanged original archive at depth 8 and 25
      MHz. Software rendering passes four held-image/source checks and ten fresh
      gameplay checks covering five-ball removal and the inspected score 50 display.
      Held-drag pacing is 60.014 guest ticks/sec; scoring replay is 60.466. WebGL captures
      confirm original drag feedback, drops, line removal and score 50 at 60.045 guest
      ticks/sec; Escape cancellation restores the source and leaves score zero at 60.155.
      No console errors; unchanged minimum 50 guest ticks/sec requirement passes.
      Browser-font reference is from a previously inspected replay, not an independent
      Macintosh emulator. All runs use a local fixture; production promotion, hosted-archive
      replay and approval are superseded by the hosted verification below.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4722
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 0bd171b0cd1fab9878501f213ac40868a36cfc34
    architecture: ppc
    environment: >-
      Launch approved after Chrome Worker replay using the real managed archive with
      no local archive fixture. Private preview app resources were served at the
      production origin through a test bridge; asset requests and production CORS
      remained unchanged. This is pre-release validation, not a live-site replay.
      Current merged compatibility code uses m68k 0.14.4 / PPC 0.7.2. Canvas2D passes
      all ten held-image, source-hole, removed-line and score-50 checks at 60.039 guest
      ticks/sec. WebGL compositor captures confirm held image, drops, five-ball removal
      and score 50 at 60.081. Both have no console errors and pass the unchanged
      minimum 50 ticks/sec. Managed archive and screenshot return HTTP 200 with exact
      pinned sizes and SHA-256 values. Human release and public replay remain pending;
      full-game, save and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4719
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: v0.97.0
    architecture: ppc
    environment: >-
      Direct public Chrome Worker replay on systemless.org/bubbles/ after the human
      release. Actual deployed app and managed archive, no private preview bridge or
      local archive fixture. Canvas2D at configured depth 8 and 25 MHz passes all ten
      held-image, source-hole, five-ball-removal and score-50 checks at 59.978 guest
      ticks/sec, above the unchanged minimum 50. Captures inspected; no console errors.
      Fresh public archive GET matches all 815666 original bytes and pinned SHA-256.
      The complete human release pipeline succeeds, including website deployment,
      six native packages and twelve nonempty package/checksum assets. Bounded public
      gameplay is qualified; full-game, save and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4692
runtime:
  executable_path: Bubbles 1.3/Bubbles1.3/Bubbles 1.3/Bubbles PPC
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 70d596f6522355282494fa6f53927f800b3971facd7ff004a96f892bcd1be677
    size_bytes: 815666
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/b/
    - https://www.vintageapplemac.com/files/games/Bubbles%201.3.sit
    - https://github.com/benletchford/systemless/issues/4692
    license: Original noncommercial shareware distribution permission
    rights_holder: Maxym Runov
    permission: >-
      Original 1.3 ReadMe explicitly permits noncommercial copying/distribution
      through Internet archives, BBSs, magazine disks and CD-ROMs without further
      permission, with only reasonable distribution/download costs. Commercial use requires
      explicit author permission. Retain the complete original package, notices and
      registration restrictions.
    notes: >-
      Unchanged original archive, 815666 bytes, SHA-256
      70d596f6522355282494fa6f53927f800b3971facd7ff004a96f892bcd1be677. StuffIt/ST46/VISE Lite package includes
      game, PP SharedLib, ReadMe, How to register, Register, Icon and original comments.
      Actual game is PPC PEF/cfrg with no CODE; installer/registration CODE does not
      prove a 68K game slice. Gratis preservation delivery separately from emulator code.
      Original $10 shareware score saving and GameField Manager locks remain; no key or
      patch. A temporary maintainer transfer supplied the same checksum-pinned original
      bytes to CI because the original host rejected GitHub downloads with 403. Normal
      promotion stored immutable managed assets; the transfer was then shut down. Edition year derives from
      the original PEF build timestamp, 1998-08-01, not a claim about the game's first
      release.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 56d495a446b69073774a56223096727897cd0ab23fffebc6058a80f1a65c704a
    size_bytes: 20096
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4692
    license: Original noncommercial shareware distribution permission
    rights_holder: Maxym Runov
    permission: Original game content accompanying the complete permitted package.
    notes: >-
      Exact original native archive at depth 8 after five-ball line removal and score
      50; cropped to the game's content, excluding desktop, main window title/frame
      and Classic Mac menu bar. No game pixels were synthesized.
references:
- https://www.vintageapplemac.com/software/games/b/
- https://github.com/benletchford/systemless/issues/4692
- https://github.com/benletchford/systemless/pull/4706
- https://github.com/benletchford/systemless/pull/4722
---

Play **Bubbles 1.3**, Maxym Runov's ball-line puzzle.
Choose **Continue** at the original colour advisory, then **File → New Game**.
Accept the Small field, then drag a ball to an empty
cell reachable through free spaces. Five matching balls in a line disappear and
add to your score; new balls arrive after other turns.

Ball movement, translucent drag feedback, line removal and scoring are verified
in native tests and the public browser player. The original unregistered
shareware restrictions remain: score saving and GameField Manager require
registration.

![Bubbles after five-ball line removal and score 50](https://assets.systemless.org/catalogue/media/sha256/56/56d495a446b69073774a56223096727897cd0ab23fffebc6058a80f1a65c704a.png)
