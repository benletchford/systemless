---
id: macsokoban
kind: game
title: MacSokoban 3.0.2
summary: >-
  Plan every push through 85 colourful warehouse puzzles in Ingemar
  Ragnemalm's polished Macintosh take on Sokoban.
developer: Ingemar Ragnemalm
publisher: Ingemar Ragnemalm
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: >-
      Deterministic headless gameplay run from the complete original
      MacSokoban 3.0.2 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2303
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: af0286028e29cb9a9c8b257dce9c75a68bbd3ae72379a29006d52f6771db82fe
    size_bytes: 91557
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/m
    - https://www.vintageapplemac.com/files/games/MacSokoban%203.0.2.sit
    license: MacSokoban non-commercial distribution permission
    rights_holder: Ingemar Ragnemalm
    permission: >-
      The built-in Legal issues documentation says the game is free of charge
      for personal use and non-commercial distribution. It retains Ingemar
      Ragnemalm's 1992-1995 copyright notice and separately restricts sale and
      commercial distribution, including the older notice concerning CD-ROM
      and other high-capacity media. This catalogue serves the complete,
      unchanged archive without charge.
    notes: >-
      Unchanged 91,557-byte StuffIt archive, SHA-256
      af0286028e29cb9a9c8b257dce9c75a68bbd3ae72379a29006d52f6771db82fe.
      It contains the game, its notes and the MacSokoban Score Mover utility.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f73d94b9880e8a657ad117313d5c9fb32d6d80f5e9624f89c93fd658a87d2d3e
    size_bytes: 9596
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2303
    permission: >-
      Original gameplay screenshot captured from the exact unchanged archive for
      this catalogue entry. Underlying game artwork remains the property of its
      rights holder.
    notes: >-
      Fresh deterministic capture after creating a named player, starting level 1
      and sending arrow-key input. The crop contains only the live puzzle surface
      and level strip; emulator menu, host margins and desktop furniture are
      excluded.
references:
- https://www.vintageapplemac.com/software/games/m
---

## Push carefully; there is no pulling

MacSokoban turns the classic warehouse puzzle into a colourful native
Macintosh game. Move the keeper through each compact room and push every
crate onto a destination square. A crate wedged against the wrong wall cannot
be pulled free, so progress depends on planning several moves ahead.

Version 3 adds multi-step undo, automatic progress saving, named players,
statistics, external level files and 85 built-in puzzles. It supports arrow
keys and direct mouse movement while retaining the fast, uncluttered feel of
the original Sokoban rules.

## Preserved as a complete non-commercial distribution

The game's built-in documentation expressly permits personal use and
non-commercial distribution while reserving commercial sale and certain
high-capacity-media distribution. Systemless therefore hosts the original
archive unchanged and without charge, preserving the notes and score-migration
utility alongside the application.

Systemless was tested beyond startup. A deterministic 68K run created a
player, entered the first puzzle, processed several arrow-key moves and showed
the keeper moving across the board without a crash, trap failure or rendering
fault.

![MacSokoban gameplay](https://assets.systemless.org/catalogue/media/sha256/f7/f73d94b9880e8a657ad117313d5c9fb32d6d80f5e9624f89c93fd658a87d2d3e.png)
