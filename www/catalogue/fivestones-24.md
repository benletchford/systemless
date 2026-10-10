---
id: fivestones-24
kind: game
title: FiveStones 2.4
summary: Place black and white stones to make five in a row on a traditional board.
developer: Xin Xu
publisher: Xin Xu
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 569c49777edbbc639a3b56816ec1ccd539814005
    architecture: 68k
    environment: >-
      Actual public v0.86.0 player, normal Chrome WebAssembly worker at 8 MHz,
      guest menus enabled. Direct hosted archive fetch returned HTTP 200,
      exact 109,973 bytes and matching SHA-256, without request interception.
      Original Not Yet, Game -> New Game -> 15-by-15, two human moves and
      computer replies produced six inspected stones. Occupied-center click
      left the consistently cropped board pixel-identical. No runtime or guest
      patch. Trial dismissal can clear menu titles until opening Game redraws
      them; issue 4395 remains open. Full browser match, saves, sound and
      higher levels remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 569c49777edbbc639a3b56816ec1ccd539814005
    architecture: ppc
    environment: >-
      Actual public v0.86.0 player, normal Chrome WebAssembly worker at 8 MHz,
      guest menus enabled. Direct hosted archive fetch returned HTTP 200,
      exact 109,973 bytes and matching SHA-256, without request interception.
      Original Not Yet, Game -> New Game -> 15-by-15, two human moves and
      computer replies produced six inspected stones. Occupied-center click
      left the consistently cropped board pixel-identical. No runtime or guest
      patch. Trial dismissal can clear menu titles until opening Game redraws
      them; issue 4395 remains open. Full browser match, saves, sound and
      higher levels remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: e3ba0f1c8d9a57f1bae9a6a7008c4e4d66598653
    architecture: 68k
    environment: >-
      Release-mode local Chrome preview, regular WebAssembly worker at 8 MHz,
      with the original guest menu bar enabled. Stationary Not Yet dismissal,
      Game -> New Game -> 15-by-15, two legal human moves and computer replies
      produced six inspected stones. An occupied-center click left the board
      unchanged. Preview archive requests used independently hash-verified original
      bytes; no runtime or guest executable patch. Full browser match, saves,
      audio and higher levels remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3976
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: e3ba0f1c8d9a57f1bae9a6a7008c4e4d66598653
    architecture: ppc
    environment: >-
      Release-mode local Chrome preview, regular WebAssembly worker at 8 MHz,
      with the original guest menu bar enabled. Stationary Not Yet dismissal,
      Game -> New Game -> 15-by-15, two legal human moves and computer replies
      produced six inspected stones. An occupied-center click left the board
      unchanged. Preview archive requests used independently hash-verified original
      bytes; no runtime or guest executable patch. Full browser match, saves,
      audio and higher levels remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3976
  - date: "2026-10-03"
    tester: Catalogue maintainer
    systemless_version: fc171b56ffed1ce48a2a48781d1504d27318dcb1
    architecture: 68k
    environment: >-
      Optimized native public-API harness linked to the portable no-default-features
      release library only, with 68K execution explicitly asserted, an 800-by-600 8-bit
      display and 32-bit addressing. Legal human placements and Level 1 computer-white
      replies completed a 14-stone game ending in a marked white five-in-row.
      Occupied, off-board and post-win placements were rejected; Undo/Redo, dialog dismissal,
      Start Over and a second 19-by-19 board worked in the covered sequence. Two clean
      processes reproduced all 27 captures exactly. This is not a complete
      GUI-executable or release-browser test. This record covers native testing only.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3976
runtime:
  show_menu_bar: true
  runtime_pacing:
    cpu_mhz: 8
  screen_depth: 8
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 9fe89beb7b8492a1c87fcb88cacd75bb8c8934c6f012bd8e52de5124bc9eea7b
    size_bytes: 109973
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/five-stones-24.hqx
    rights_holder: Xin Xu
    permission: >-
      The bundled README_FIRST permits non-profit distribution with that document
      attached without any modifications. Distribution for other purposes, explicitly
      including CD-ROM, requires written author permission. This complete original package
      retains the application and unchanged README_FIRST; no permission for commercial
      or other-purpose distribution is asserted.
    notes: >-
      Original 109,973-byte Info-Mac BinHex/StuffIt package. The README identifies
      version 2.4, copyright 1993-1995, and the FAT build. Both 68K CODE resources and a
      PowerPC code fragment are present; both slices have bounded browser gameplay checks. This is
      original $10 shareware, not freeware or a registered/unlocked copy. The authentic
      shareware notice was dismissed using Not Yet, without registration or game patches.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: e3e403679514942e7e43e975a0949e0662b8b8c46ff1859d870fdec69b954cd0
    size_bytes: 2593
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3976
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original shareware package
      for this non-profit catalogue entry. Underlying game artwork remains its owner's
      property.
    notes: >-
      Authentic native framebuffer after six legal black and six white stones, with
      numbered stones enabled. Exact 320-by-320 content crop at (4,40) from the
      800-by-600 framebuffer; no rescaling or retouching. Host framing and the Classic Mac menu
      and window title bars are excluded. This is not a release-browser capture.
- id: win-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: eddb7b5557b2899a23ef5ad4cfac3fd7f0dba3fd1cf68dc3f90ecc12160319b0
    size_bytes: 2732
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3976
    permission: >-
      Fresh Systemless gameplay capture from the unchanged original shareware package
      for this non-profit catalogue entry. Underlying game artwork remains its owner's
      property.
    notes: >-
      Authentic native framebuffer showing the computer's five triangle-marked white
      stones after a natural 14-stone game. Exact 320-by-320 content crop at (4,40)
      from the 800-by-600 framebuffer; no rescaling or retouching. Host framing and the
      Classic Mac menu and window title bars are excluded. This is not a release-browser
      capture.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/brd/five-stones-24.hqx
---

## Five in a row

![FiveStones after native human moves and computer replies](https://assets.systemless.org/catalogue/media/sha256/e3/e3e403679514942e7e43e975a0949e0662b8b8c46ff1859d870fdec69b954cd0.png)

Choose **Game → New Game** for a 15-by-15 or 19-by-19 board. The first black
stone is placed in the centre automatically. Click an empty intersection to
place a stone; the first side to make five or more consecutive stones across,
down or diagonally wins. **Play** selects the opponent mode, and **Level**
selects the computer's strength. **Undo Move**, **Redo Move** and **Start Over**
let you revisit or restart the game.

Xin Xu's original **$10 shareware** package includes its unchanged
README_FIRST and retains the original registration state. Its terms permit
intact non-profit distribution with that document attached unmodified.
Distribution for other purposes, explicitly including CD-ROM, requires written
author permission.

## Verification status

![FiveStones native white-computer five-in-row](https://assets.systemless.org/catalogue/media/sha256/ed/eddb7b5557b2899a23ef5ad4cfac3fd7f0dba3fd1cf68dc3f90ecc12160319b0.png)

Native checks covered a complete game against the Level 1 white computer,
legal placements and replies, invalid occupied and off-board clicks,
Undo/Redo, a visibly marked white diagonal win, and rejected placements after
the win. The completed board stayed unchanged for another 600 no-input ticks.
Dialogs dismissed cleanly, Start Over reset the board, and a second 19-by-19
game accepted a move and computer reply. Two clean processes reproduced all
27 captured frames exactly. Both screenshots come from that native run.

Release-mode browser checks on both 68K and PowerPC cover the original trial
notice, a new 15-by-15 board, two human moves with computer replies, and an
occupied click that leaves the board unchanged. The guest menu bar is enabled
so **Game → New Game** remains available. Trial dismissal can clear the menu
titles; click beside the small application icon at the upper left to open
**Game** and redraw the menu. PowerPC can also retain an empty trial-window
outline until a game starts. These redraw issues remain tracked separately.

The full-match checks above used an optimized portable native release-library
harness, not the complete GUI executable. Full browser matches, cross-process
preferences, save/load round trips, printing, sound, higher AI levels,
alternate playing modes and third-ply restriction were not tested. Saving
current defaults carried numbered stones into the next game in the same
process; persistence after restarting the application is unverified.
