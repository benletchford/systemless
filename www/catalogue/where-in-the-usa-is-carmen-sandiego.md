---
id: where-in-the-usa-is-carmen-sandiego
kind: game
title: "Where in the U.S.A. Is Carmen Sandiego?"
summary: >-
  Follow Carmen's gang across America in Brøderbund's playable Macintosh
  demonstration.
developer: Brøderbund Software, Inc.
publisher: Brøderbund Software, Inc.
year: 1989
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic new-detective and sample-case run from the unchanged
      cover-disc StuffIt demo, with its animated tour cross-checked under
      BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2423
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/where-in-the-usa-is-carmen-sandiego/Carmen-USA-Demo.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/where-in-the-usa-is-carmen-sandiego
    license: Brøderbund Carmen U.S.A. promotional demo distribution
    rights_holder: Brøderbund / The Learning Company and their successors
    permission: >-
      Brøderbund deliberately distributed this self-contained promotional demo
      on contemporary magazine and software-library discs. This entry preserves
      that exact unchanged demo archive; it does not include or claim permission
      for the retail game or any other Carmen Sandiego title.
    notes: >-
      Unchanged 291,100-byte StuffIt archive, SHA-256
      c26af580aae0b0c60eb0f29700106b3c45baa05217a4cdbd096ab92a1cc8f20d.
      It contains one runnable 68K application named “Carmen USA™ (Demo)” plus
      its original music files.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/where-in-the-usa-is-carmen-sandiego/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2423
    permission: >-
      Original gameplay screenshot captured for this catalogue at the
      maintainer's request. Underlying Carmen Sandiego artwork remains the
      property of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the exact
      unchanged demo archive on 2026-09-23 during its interactive sample-game
      introduction. The complete 800x600 guest framebuffer is preserved without
      alteration or host chrome. PNG SHA-256
      08a5d8c2b613f76cc776bc71609d6aa171c9c4bd86a3425dbf1c519cf7f239de,
      94,283 bytes.
references:
- https://classicmacdemos.com/where-in-the-usa-is-carmen-sandiego
---

## Chase the gang across America

![Where in the U.S.A. Is Carmen Sandiego? demo](incoming/where-in-the-usa-is-carmen-sandiego/gameplay.png)

A stolen monument starts a chase across all fifty states. Witnesses reveal
geographic clues, travel choices consume precious time, and the detective has
to identify the suspect before making an arrest. The familiar Carmen Sandiego
loop turns maps, landmarks and state history into the tools of an investigation.

The demonstration introduces that loop with a guided national tour and a
playable sample case. It creates a named detective, assigns a missing monument,
and opens the original Game and Police Dossiers menus for the pursuit.

## Brøderbund's cover-disc demonstration

This is the original `Carmen USA Demo`, not the retail game and not *Where in
the World Is Carmen Sandiego?*. It is included as the closest legally
distributable period demonstration for that target-list slot. The preserved
package was deliberately circulated on at least two contemporary software
discs and identifies itself throughout as a demo.

Systemless opens the unchanged StuffIt archive, plays the animated tour,
accepts a detective name and begins the supplied sample assignment. A
deterministic run reaches that playable case, while BasiliskII independently
confirms the same archive's startup and tour sequence.
