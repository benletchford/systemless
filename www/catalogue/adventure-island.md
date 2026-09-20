---
id: adventure-island
kind: game
title: Adventure Island
summary: >-
  Explore a mysterious island and survive a shipwreck in Kim R.'s compact
  Macintosh text adventure.
developer: Kim R.
year: 1998
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
    systemless_version: "0.45.0"
    architecture: 68k
    environment: >-
      Deterministic command-parsing run from the complete unchanged Adventure
      Island archive using its 68k Macintosh application path
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2321
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Adventure%20Island.sit
    download_page: https://www.vintageapplemac.com/software/games/a/
    expected_sha256: bd20a3ce8bbd285ac9e3f84c620380a7fe027dd17aa04a0e88f45f71241ad635
    expected_size: 131259
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/a/
    - https://www.vintageapplemac.com/files/games/Adventure%20Island.sit
    license: Adventure Island ReadMe explicit unchanged distribution permission
    rights_holder: Kim R.
    permission: >-
      The bundled Adventure Island ReadMe states: “This Game is Freeware. Go
      ahead and put it on download sites or CD roms or whatever, but please
      don't alter anything.” This entry therefore references the complete,
      unchanged archive and does not modify its contents.
    notes: >-
      Unchanged 131,259-byte StuffIt archive, pinned by SHA-256
      bd20a3ce8bbd285ac9e3f84c620380a7fe027dd17aa04a0e88f45f71241ad635. The
      complete package retains the Adventure Island 68k application and its
      Adventure Island ReadMe.
references:
- https://www.vintageapplemac.com/software/games/a/
- https://www.vintageapplemac.com/files/games/Adventure%20Island.sit
- https://github.com/benletchford/systemless/issues/2321
---

## A text adventure after the shipwreck

Adventure Island casts the player as a survivor of a hurricane, washed toward a
mysterious island. The compact 68k text adventure accepts directional and
object commands, with mouse input available for examining or collecting things.

## Preserved under the ReadMe's distribution permission

The bundled ReadMe says: “Go ahead and put it on download sites or CD roms or
whatever, but please don't alter anything.” This entry points to the exact
unchanged 131,259-byte archive and keeps its application and ReadMe together.

## Live command parsing

Systemless reaches the text-adventure window from the exact archive. A
deterministic run clicked the command area, entered `n`, pressed Return, and
received the in-game response `You can't go that way.` This verifies input
processing beyond launch or a static title screen. See [issue #2321](https://github.com/benletchford/systemless/issues/2321)
for the reproducibility record.
