---
id: matchmaker
kind: game
title: MatchMaker 1.0
summary: >-
  Find the matching pair—or the one tile without a partner—across colourful,
  freshly generated picture boards.
developer: Mark Pilgrim
publisher: Mark Pilgrim
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
    systemless_version: "0.42.4"
    architecture: 68k
    environment: >-
      Deterministic headless gameplay run from the complete original MatchMaker 1.0
      source distribution
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2287
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 9b1e034b0b2eaed8f75d32a7d9011fbbfa0b3f78eb67c789befa8e856a346153
    size_bytes: 185432
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/m
    - https://www.vintageapplemac.com/files/games/MatchMaker%201.0%20source.sit.sit
    license: GPL-2.0-or-later
    rights_holder: Mark Pilgrim and contributors
    permission: >-
      The included legal notice permits redistribution and modification under GNU GPL
      version 2 or any later version. The unchanged distribution includes the runnable
      application, complete source tree and full GPL v2 text together, while
      preserving the attribution for Jim's CDEFs.
    notes: >-
      Unchanged 185,432-byte outer StuffIt archive, SHA-256
      9b1e034b0b2eaed8f75d32a7d9011fbbfa0b3f78eb67c789befa8e856a346153. It contains the original 192,639-byte
      nested MatchMaker 1.0 source archive, whose SHA-256 is
      67eb15af29fe707b13342bce203bd6b1e082fd7b5a78e34c854b02e874814d2a.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7772a2373b0e7a9d89cbbad5dbcd5dfa2d6e512dcd9d470bdde2026558d57fca
    size_bytes: 2532
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://www.vintageapplemac.com/software/games/m
    - https://www.vintageapplemac.com/files/games/MatchMaker%201.0%20source.sit.sit
    - https://github.com/benletchford/systemless/issues/2335
    permission: >-
      Original gameplay screenshot captured from the exact unchanged archive for this
      catalogue entry. Underlying puzzle artwork remains the property of Mark Pilgrim
      and contributors.
    notes: >-
      Fresh deterministic Systemless capture after completing first-run setup,
      requesting a new game and generating the live 5x5 picture board. The 800x600
      framebuffer was cropped tightly to the 221x221 board surface, excluding the Mac menu bar,
      desktop, cursor and emulator chrome. PNG SHA-256
      7772a2373b0e7a9d89cbbad5dbcd5dfa2d6e512dcd9d470bdde2026558d57fca, 2,532 bytes.
references:
- https://www.vintageapplemac.com/software/games/m
---

## A new visual puzzle every time

MatchMaker fills a board with small, colourful pictures and asks you to spot
the relationship hidden among them. Depending on the selected mode, exactly one
pair matches or one picture is the only tile without a partner. Each new game
generates a fresh arrangement, turning a simple visual premise into a quick test
of scanning, memory and attention.

Difficulty presets and board sizes ranging from a compact 5-by-5 grid to a
large 15-by-21 challenge let the puzzle scale from a brief diversion to a dense
search. The deliberately quiet design provides no victory fanfare: identifying
and selecting the correct tiles is its own conclusion.

## Preserved with buildable source

The original release is a source distribution as well as a game. Its nested
archive contains the 68K application, project source, resources, documentation,
GNU GPL version 2 and contributor attribution. This entry retains the complete
double-wrapped package so every recipient receives the same source and licence
materials as the executable.

Systemless was tested from the original outer archive rather than an extracted
copy. A deterministic run completed the first-run setup, generated a visible
5-by-5 board and selected the identified matching pair without a trap, crash or
rendering failure.

![MatchMaker gameplay](https://assets.systemless.org/catalogue/media/sha256/77/7772a2373b0e7a9d89cbbad5dbcd5dfa2d6e512dcd9d470bdde2026558d57fca.png)
