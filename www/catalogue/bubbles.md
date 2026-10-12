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
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: fb3e11d3b67895a21e3dae7b05ca7fba4615fb12
    architecture: ppc
    environment: >-
      Fresh optimized native build, untouched original 1.3 archive and exact PPC
      executable selector below, depth 16. Ordinary File, New Game and OK render the
      Small board. Original masked translucent drag image, ball drops, five-ball
      removal and score 50 pass thirty measured pixel assertions at 2050 frontend /
      2021 guest ticks, zero exhausted frames. Captures inspected. Thirty-seven
      drag-related tests pass. A separate nine-assertion replay and cell/drawing
      traces confirm source clearing followed by new-ball repopulation. Six inner
      VISE files/twelve forks and type/creator records match an independently
      implemented pinned ScummVM-reference reader. Original Finder flags are not
      recovered by that reference; original archive bytes remain unchanged. Full
      games, saves and audio fidelity remain unverified. Local Chrome Worker replays
      at depth 8 show board rendering, ball drops and score 50, with WebGL at
      59.959 and Canvas2D at 59.996 guest ticks per second and no console errors.
      Both use the exact local archive fixture, not a hosted production archive.
      Held drag feedback is absent in both rendering paths; direct rendered-canvas
      captures confirm the discrepancy with the native image. These observations
      do not qualify browser gameplay. Browser assertions, asset promotion and
      launch approval remain pending; no human release is asserted.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4692
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: fb3e11d3b67895a21e3dae7b05ca7fba4615fb12
    architecture: ppc
    environment: >-
      Fresh native replay at the configured browser-compatible depth 8, untouched
      original archive and exact selector. Dismiss the original 256-colour advisory
      with Continue, then normal New Game/OK, held translucent image, ball drops,
      five-ball removal and score 50 pass thirty measured pixel assertions at 2176
      frontend / 2148 guest ticks, zero exhausted frames. Captures inspected. The
      initial inspection without dismissing the advisory only showed that dialog
      and did not qualify gameplay. Browser and launch approval remain pending.
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
    type: url
    url: https://www.vintageapplemac.com/files/games/Bubbles%201.3.sit
    expected_sha256: 70d596f6522355282494fa6f53927f800b3971facd7ff004a96f892bcd1be677
    expected_size: 815666
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
      permission, with only reasonable distribution/download costs. Commercial use
      requires explicit author permission. Retain the complete original package,
      notices and registration restrictions.
    notes: >-
      Unchanged original archive, 815666 bytes, SHA-256
      70d596f6522355282494fa6f53927f800b3971facd7ff004a96f892bcd1be677.
      StuffIt/ST46/VISE Lite package includes game, PP SharedLib, ReadMe, How to
      register, Register, Icon and original comments. Actual game is PPC PEF/cfrg
      with no CODE; installer/registration CODE does not prove a 68K game slice.
      Gratis preservation delivery separately from emulator code. Original $10
      shareware score saving and GameField Manager locks remain; no key or patch.
      Edition year derives from the original PEF build timestamp, 1998-08-01,
      not a claim about the game's first release.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/bubbles/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4692
    license: Original noncommercial shareware distribution permission
    rights_holder: Maxym Runov
    permission: Original game content accompanying the complete permitted package.
    notes: >-
      Exact original native archive at depth 8 after five-ball line removal and score 50;
      cropped to the game's content, excluding desktop, main window title/frame
      and Classic Mac menu bar. No game pixels were synthesized.
references:
- https://www.vintageapplemac.com/software/games/b/
- https://github.com/benletchford/systemless/issues/4692
- https://github.com/benletchford/systemless/pull/4706
---

Play **Bubbles 1.3**, Maxym Runov's ball-line puzzle.
Choose **Continue** at the original colour advisory, then **File → New Game**.
Accept the Small field, then drag a ball to an empty
cell reachable through free spaces. Five matching balls in a line disappear and
add to your score; new balls arrive after other turns.

Native ball movement, translucent drag feedback, line removal and scoring are
verified. Browser launching is pending approval. The original unregistered
shareware restrictions remain: score saving and GameField Manager require
registration.

![Bubbles after five-ball line removal and score 50](incoming/bubbles/gameplay.png)
