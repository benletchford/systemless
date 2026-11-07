---
id: gnu-chess-mac
kind: game
title: GNU Chess Mac 4.0b5
summary: >-
  Play chess against the Macintosh port of GNU Chess, with an opening book,
  adjustable clocks and a move list.
developer: Dan Oetting and contributors
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete original 4.0b5 BinHex archive, exact executable selector, explicit 68K
      and 800x600/depth 8. A click in the board footer activates its initially
      inactive window. Ordinary drag e2-e4 moves the white pawn; the computer answers e7-e5
      and both moves appear in the list. The equal-duration omitted-drag control keeps
      both pawns at their starting squares and the move list empty. Fresh active/control
      repeats each pass eight measured pixels and three exact earlier Systemless
      captures, eleven checks at 418 frontend/1018 guest ticks, zero exhaustion. Own-run
      references establish consistency, not hardware-oracle accuracy. Runtime Rust/Cargo
      source matches merged 2a0e56f6 and 3778436f. Native binary SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. The intact executable also
      contains PPC code, but its tested PPC slice opens a black board and is not exposed as
      supported. Browser input and pacing, complete games, save/load and sound remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2290
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: b3c66dbcd7beaacad9e8b7496eca0964bd16ee29 private browser candidate
    architecture: 68k
    environment: >-
      Optimized private HTTPS frontend on current merged runtime 3778436f,
      Chromium 156.0.8078.79/Linux aarch64, isolated 68K worker/WebGL,
      800x600/depth 8, 25 MHz. Deployed COEP credentialless/COOP same-origin
      and SharedArrayBuffer verified. Actual untouched hosted archive HTTP 200
      response matches 232547 bytes and its SHA-256. Both main and worker Wasm
      response bodies match SHA-256 d02dea4b18c28423f23a2a65b2049525a847a2b0f8e22c8120864160490e31fd,
      17760277 bytes. Required ordinary Options-Easy selection disables pondering
      on the human turn. Trusted drags play e2-e4 and Ng1-f3; computer replies
      e7-e5 and Nb8-c6. Matched Easy control omits both drags and preserves the
      initial board and empty move list. Each unchanged 20-second pacing sample
      passes: active/control final steady guest 57.8469/59.8970 and host
      59.8766/59.8970 per second, max frames 18.9/19.1 ms, minimum audio queue
      131.1/130.8 ms, zero captured lifecycle/network errors. These final steady
      snapshots are not sample-wide means. Default pondering mode plays both
      moves but fails pacing at 37.6242 guest ticks/sec; approval requires Easy.
      Complete games, deeper search, persistence, sound fidelity, PPC and released
      public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2290
runtime:
  executable_path: GnuChessMac40b5ƒ/GnuChessMac40b5
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 346158a881746632ff6ec74eefab43bc0fc1afea503725645f32a712fed5f0e7
    size_bytes: 232547
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.tim-mann.org/gnuchess.html
    - https://www.tim-mann.org/gnuchess/gnuchessMAC40b5.hqx
    - https://www.gryphel.com/c/sw/games/chess/
    license: GPL-2.0-only
    rights_holder: GNU Chess authors and Macintosh port contributors
    permission: >-
      The retained ReadMe explicitly identifies GNU licensing and free distribution;
      the complete original COPYING supplies GNU GPL version 2. Preserve the entire
      unchanged package, licence and notices. The complete matching original source
      archive is supplied as a managed supplement alongside this binary.
    notes: >-
      Original MD5 2a5accfbf4598956b4e07c173abac4b9. All retained binary and
      matching-source data/resource notices were reviewed. Independent production packing
      matches all five paths, ten forks and five Finder records against unar/lsar with
      strict end validation. The original fat executable has CODE 0-3, cfrg 0 and a PowerPC
      PEF data fork; only 68K is presently qualified.
- id: source-code
  role: supplement
  format: hqx
  source:
    type: sha256
    sha256: 20883c771663d7c159aa764d9983371f70d3c08184e4a1839aff9a987f538fb2
    size_bytes: 211850
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.tim-mann.org/gnuchess.html
    - https://www.tim-mann.org/gnuchess/gnuchessMAC-src-v40b5.hqx
    license: GPL-2.0-only
    rights_holder: GNU Chess authors and Macintosh port contributors
    permission: >-
      Complete matching original Mac 4.0b5 source, including C sources, Macintosh UI,
      projects, bitmap resources and COPYING. Retain the unchanged archive alongside
      the binary. Source notices affirm GPL copying; source and binary COPYING documents
      are identical complete GPLv2 texts.
    notes: >-
      Original MD5 0fd91dbf63f9ea8a286a4fab3a1cb43a. Independent production packing
      matches all 39 source paths, 78 forks and 39 Finder records against unar/lsar,
      including resource-only project files, with strict end validation.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8e3b836aae10e041cafe69e569e40a2602087a36d1c3c5d2d641487be1234972
    size_bytes: 5275
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2290
    license: GPL-2.0-only
    rights_holder: GNU Chess authors and Macintosh port contributors
    permission: Original gameplay capture of the unchanged GPL-licensed game.
    notes: >-
      Exact 272x272 board crop at (182,45), after e2-e4 and the computer's e7-e5
      reply. No resampling, menu bar, window framing or host interface.
references:
- https://www.tim-mann.org/gnuchess.html
- https://www.gryphel.com/c/sw/games/chess/
- https://github.com/benletchford/systemless/issues/2290
---

![GNU Chess after e2-e4 and the computer's e7-e5 reply](https://assets.systemless.org/catalogue/media/sha256/8e/8e3b836aae10e041cafe69e569e40a2602087a36d1c3c5d2d641487be1234972.png)

Select **Options → Easy** before playing so the computer thinks only during
its own turn. Click the chessboard window to activate it, then drag a white piece to a legal
square. The Macintosh plays black. **File → New Game** resets the board;
**Players** selects which side the computer plays. Castle by dragging the king
two squares. **Edit → Undo** takes back your last move.

This original release is free software under GNU GPL version 2.
[Download the complete matching original source](https://assets.systemless.org/catalogue/objects/sha256/20/20883c771663d7c159aa764d9983371f70d3c08184e4a1839aff9a987f538fb2.hqx).

Native 68K play verifies one legal move and its reply. Browser play with
**Easy** selected verifies two moves and replies against an omitted-input control.
Default pondering mode runs below the pacing minimum. Full games, deeper search,
save/load, sound fidelity and released public play remain unverified. The original
also contains PPC code; that slice currently displays a black board.
