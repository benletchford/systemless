---
id: crafty-chess-11-13
kind: game
title: Crafty Chess 11.13
summary: >-
  Play against Robert Hyatt's Crafty chess engine in Rolf Exner's native
  Macintosh port, with its text prompt, AppleEvent support and opening book.
developer: Robert M. Hyatt and Rolf Exner
year: 1996
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
      Deterministic realtime-tick run from the original Crafty Chess 11.13
      BinHex/StuffIt distribution, entering d followed by Return at the
      Crafty prompt
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2293
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 856d578f6baea59f88682e7da81bd3ea0b1f24886d2b8178f417eebeeae9f83e
    size_bytes: 972415
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/misc/crafty-chess-1113.hqx
    - https://craftychess.com/downloads/source/crafty-11.13.zip
    license: Crafty noncommercial source-available license
    rights_holder: Robert M. Hyatt; Macintosh port by Rolf Exner
    permission: >-
      The matching 11.13 source header reserves all rights and prohibits
      reproducing any part of Crafty for commercial (for-profit or sale)
      reasons. It permits the program to be freely distributed, used and
      modified only when that use does not result in selling any part of the
      source, executable or other distributed material in the package. This
      catalogue copy is therefore limited to unchanged, noncommercial
      redistribution with the matching source and terms preserved.
    notes: >-
      Unchanged 972,415-byte BinHex archive, SHA-256
      856d578f6baea59f88682e7da81bd3ea0b1f24886d2b8178f417eebeeae9f83e.
      The included README identifies Rolf Exner's December 1996 Macintosh port
      as based on Crafty 11.13 and describes it as a fat binary; this catalogue
      declares only the 68k path verified by Systemless.
- id: source-code
  role: supplement
  format: zip
  source:
    type: external
    url: https://web.archive.org/web/20170315123002id_/http://craftychess.com/downloads/source/crafty-11.13.zip
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://craftychess.com/downloads/source/crafty-11.13.zip
    - https://web.archive.org/web/20170315123002id_/http://craftychess.com/downloads/source/crafty-11.13.zip
    license: Crafty noncommercial source-available license
    rights_holder: Robert M. Hyatt
    permission: >-
      The source header for Crafty 11.13 reserves all rights and permits free
      distribution, use and modification only when no part of the source,
      executable or other distributed material is sold. Commercial (for-profit
      or sale) reproduction is expressly excluded; this exact source archive
      is supplied as the corresponding source for the unchanged Macintosh
      distribution under those noncommercial terms.
    notes: >-
      Exact upstream 264,580-byte source archive, SHA-256
      58665bf510f9339c44d5298729ce42a96639c01064a5c748c0a0384cc4019583.
      The archive's main.c header is the rights evidence for the source and
      executable distribution terms.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7a512b4937e7dc5c9a124f615e2e936f53ea7e0bd542439baf906f85928002c4
    size_bytes: 5348
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2293
    permission: >-
      Original gameplay screenshot captured from the exact unchanged archive for
      this catalogue entry. Underlying chess artwork remains the property of its
      rights holders.
    notes: >-
      Fresh deterministic realtime-tick capture after entering d and Return at
      the Crafty prompt. The crop contains only the Crafty text output and live
      ASCII board content; emulator menu, host margins and desktop furniture are
      excluded.
references:
- https://www.vintageapplemac.com/files/misc/crafty-chess-1113.hqx
- https://craftychess.com/downloads/source/crafty-11.13.zip
- https://web.archive.org/web/20170315123002id_/http://craftychess.com/downloads/source/crafty-11.13.zip
- https://github.com/benletchford/systemless/issues/2293
---

## Crafty on the Macintosh

Crafty Chess 11.13 brings Robert M. Hyatt's long-running chess engine to a
native Macintosh interface. Rolf Exner's port keeps Crafty's command-driven
workflow while adding AppleEvent support for graphical front ends such as
ExaChess and a bundled opening book. At the prompt, `d` (or `display`) prints
the current board, while ordinary algebraic moves let Crafty search and reply.

## The original release and its source

The catalogue preserves the unchanged BinHex archive and the exact matching
11.13 source supplement. The source header is explicit that Crafty is not for
commercial reproduction: source, executables and other distributed materials
may be freely distributed, used and modified only when no part is sold. The
source archive is included as the corresponding rights evidence, while this
catalogue entry is limited to noncommercial redistribution.

The upstream README describes the Macintosh application as a fat binary. Only
its 68k path is declared here because that is the architecture verified in
Systemless; no PPC compatibility claim is made.

## Verified prompt and board interaction

With realtime-tick pacing enabled, a deterministic run reached the Crafty
prompt, entered `d` followed by Return, and produced the echoed `White(1): d`
line together with the ASCII chess board. This is the interaction recorded in
[issue #2293](https://github.com/benletchford/systemless/issues/2293).

![Crafty Chess gameplay](https://assets.systemless.org/catalogue/media/sha256/7a/7a512b4937e7dc5c9a124f615e2e936f53ea7e0bd542439baf906f85928002c4.png)
