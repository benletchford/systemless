---
id: gnu-chess-4
kind: game
title: GNU Chess Mac 4.0b5
summary: >-
  Play a complete game of chess against GNU Chess 4.0b5 in Dan Oetting's
  Macintosh interface, with the original opening book and matching source.
developer: Free Software Foundation and GNU Chess contributors
publisher: Free Software Foundation
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.43.0"
    architecture: 68k
    environment: >-
      Deterministic headless gameplay run from the original GNU Chess Mac 4.0b5
      BinHex archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2290
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
    - https://www.gryphel.com/c/sw/games/chess/
    - https://www.gryphel.com/d/sw/games/chess/c/gnuchessMAC40b5.hqx
    license: GNU General Public License v2
    rights_holder: Free Software Foundation and GNU Chess contributors; Macintosh port by Dan Oetting, Tom Gerardy, and Airy André
    permission: >-
      The unchanged distribution includes COPYING, the GNU General Public License
      (June 1991), alongside the executable, opening book and documentation. The GPL
      permits copying and redistribution when the licence and corresponding source are
      preserved.
    notes: >-
      Untouched 232,547-byte Gryphel BinHex archive, SHA-256
      346158a881746632ff6ec74eefab43bc0fc1afea503725645f32a712fed5f0e7. The archive
      contains the GNU Chess Mac 4.0b5 fat application, opening book, README and COPYING.
      Its README records the 4.0b5 release and both 68K and PowerPC slices; only the
      68K slice is exposed here because that is the verified Systemless path.
- id: source
  role: supplement
  format: hqx
  source:
    type: external
    url: https://www.gryphel.com/d/sw/games/chess/c/gnuchessMAC-src-v40b5.hqx
  provenance:
    redistribution: permitted
    sources:
    - https://www.gryphel.com/c/sw/games/chess/
    - https://www.gryphel.com/d/sw/games/chess/c/gnuchessMAC-src-v40b5.hqx
    license: GNU General Public License v2
    rights_holder: Free Software Foundation and GNU Chess contributors; Macintosh port by Dan Oetting, Tom Gerardy, and Airy André
    permission: >-
      The source archive includes the complete GNU General Public License (June 1991)
      and the C source, headers and Macintosh project files corresponding to the
      4.0b5 application. The GPL permits copying and redistribution of this source.
    notes: >-
      External, unmodified 211,850-byte Gryphel BinHex source archive, SHA-256
      20883c771663d7c159aa764d9983371f70d3c08184e4a1839aff9a987f538fb2. The source's
      version.h identifies version 4.0 patchlevel b5; Chess68K.µ and Chess.µ project
      files list the same Macintosh source modules used by the binary. Its COPYING is
      byte-identical to the binary archive's COPYING (17,976 bytes,
      SHA-256 bf0382942b51d24490ad2ecc0c2ae6be6340eca3ffb5e2cc31910ae519d96792),
      providing corresponding-source and licence evidence for this exact release.
references:
- https://www.gryphel.com/c/sw/games/chess/
- https://www.gryphel.com/d/sw/games/chess/c/gnuchessMAC40b5.hqx
- https://www.gryphel.com/d/sw/games/chess/c/gnuchessMAC-src-v40b5.hqx
---

## GNU Chess on a classic Macintosh board

GNU Chess Mac 4.0b5 brings the GNU chess engine to a native Macintosh board and
move-list interface. The 1995 Gryphel distribution includes the original opening
book, documentation and a fat application containing both classic 68K and PowerPC
code. This catalogue exposes the tested 68K slice; the PowerPC slice remains outside
the verified compatibility claim.

## A real game, not just a launch

Systemless was tested beyond startup from the unchanged archive. A deterministic
headless run accepted the legal human move **e2-e4**, updated the board and move list,
and then let the engine answer **d7-d6**. The run reached live play without a trap or
crash; the exact test and screenshots are recorded in [issue #2290](https://github.com/benletchford/systemless/issues/2290).

## Matching GPL source

The companion source archive is kept as an external supplement at its original Gryphel
URL rather than being silently substituted with a later GNU Chess release. Its
`version.h` says 4.0/b5, its 68K and PowerPC project files name the Macintosh source
modules, and its GPL text is byte-identical to the copy shipped beside the binary.
Together those records identify the source as the corresponding GPL source for this
specific 4.0b5 Macintosh build.
