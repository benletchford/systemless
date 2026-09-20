---
id: asterax
kind: game
title: Asterax
summary: >-
  Blast asteroids for emeraldium in Arvandor Software's colorful strategic arcade
  game for one or two pilots.
developer: Arvandor Software
publisher: Arvandor Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: Deterministic headless gameplay run from the unchanged Asterax archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2305
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 78bb57247d88f731426aaaa60e501b5cb41bc8d484268c5ff78fab20b70340d8
    size_bytes: 401668
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/
    - https://www.vintageapplemac.com/files/games/Asterax.sit
    license: Asterax non-profit distribution grant
    rights_holder: Arvandor Software
    permission: >-
      The bundled Asterax README permits free distribution when the README and all
      standard application files remain included. It prohibits distribution for
      profit without Arvandor Software's consent, so this entry is suitable only
      for non-profit hosting and redistribution of the complete standard package.
    notes: >-
      Unchanged 401668-byte StuffIt archive, pinned by SHA-256
      78bb57247d88f731426aaaa60e501b5cb41bc8d484268c5ff78fab20b70340d8.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/asterax/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2305
    permission: >-
      Original screenshot captured from the exact Asterax archive during a
      deterministic gameplay run for this catalogue. Underlying game artwork
      remains the property of its rights holders.
    notes: >-
      Fresh capture after ship selection, fire, thrust and turn input, using
      the full 800-by-600 Asterax gameplay surface. It excludes emulator chrome,
      host margins and the Classic Mac menu bar.
references:
- https://www.vintageapplemac.com/software/games/
- https://www.vintageapplemac.com/files/games/Asterax.sit
---

## Source and version

This entry uses the unchanged Asterax 1.0.1 StuffIt archive. The bundled README
and all standard application files remain together so the archive preserves the
author's non-profit distribution condition.

## In the asteroid field

![Asterax gameplay](incoming/asterax/gameplay.png)

Asterax begins with a ship-selection screen and opens into a fast asteroid field
where pilots mine emeraldium while avoiding rival ships and hazards. The original
Player One controls are KP8 for thrust, KP6 and KP4 for turning, and Control for
fire. A deterministic Systemless run selected a ship, entered the asteroid field,
and verified visible state changes after fire, thrust and turn input.

## Distribution condition

Arvandor Software's bundled README grants free distribution only when the README
and all standard application files are included. Distribution for profit requires
the author's consent; this catalogue entry therefore describes non-profit use and
does not grant commercial redistribution rights.
