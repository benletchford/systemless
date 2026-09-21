---
id: xconq
kind: game
title: Xconq
summary: >-
  Command armies across dozens of historical, fantastic and experimental
  turn-based scenarios.
developer: Stan Shebs and contributors
publisher: Cygnus Support
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
    systemless_version: "0.46.0"
    architecture: 68k
    environment: >-
      Deterministic headless gameplay run from the original Xconq 7.0.1 Macintosh
      archive with the Classic Mac menu bar hidden from the capture
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2353
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: d83869c255a782e2df466f1fb5d0bcfa3dd262af6709b463bde629000d1d5585
    size_bytes: 1066669
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/x
    - https://www.vintageapplemac.com/files/games/Xconq%207.0.1.sit
    - https://ibiblio.org/pub/Linux/games/multiplayer/xconq-7.0.1.src.tgz
    license: GPL-2.0-only
    rights_holder: Stan Shebs and contributors
    permission: >-
      The package includes the GNU General Public License version 2 and identifies
      Xconq as free software licensed under that license. Redistribution is permitted
      when recipients receive the same rights and corresponding source code remains
      available. This entry therefore preserves the unmodified binary together with the
      matching complete 7.0.1 source distribution.
    notes: >-
      Unchanged 1,066,669-byte StuffIt archive, SHA-256
      d83869c255a782e2df466f1fb5d0bcfa3dd262af6709b463bde629000d1d5585.
- id: source-code
  role: supplement
  format: gz
  source:
    type: external
    url: https://ibiblio.org/pub/Linux/games/multiplayer/xconq-7.0.1.src.tgz
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ibiblio.org/pub/Linux/games/multiplayer/xconq-7.0.1.src.tgz
    license: GPL-2.0-only
    rights_holder: Stan Shebs and contributors
    permission: >-
      Xconq 7.0.1's README licenses the program under GNU GPL version 2 and permits
      copying, modification and redistribution. This is the matching complete upstream
      source archive for the hosted Macintosh binary.
    notes: >-
      Unchanged 1,457,741-byte gzip-compressed source archive, SHA-256
      481939e53183699b5a0aa8bea68e85a0284ab9e6b94ff458fc8fe0de3a3ee0a6.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7de2e09ab4d9f0aac3423f19db43b45a9b27930ff69d6bcc4a2c5bac12d2a4ad
    size_bytes: 174470
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2353
    permission: >-
      Original gameplay screenshot captured from the exact unchanged archive for this
      catalogue entry. Underlying game artwork remains the property of its rights
      holders.
    notes: >-
      Fresh deterministic Systemless capture of a live introductory scenario with the
      hex map, player panels and instructions visible. The 800-by-600 capture excludes
      the Classic Mac menu bar, browser, host desktop and emulator chrome.
references:
- https://www.vintageapplemac.com/software/games/x
- https://ibiblio.org/pub/Linux/games/multiplayer/xconq-7.0.1.src.tgz
- https://github.com/benletchford/systemless/issues/2353
---

## A strategy-game construction kit

Xconq is both a turn-based strategy game and a system for building new ones.
Version 7.0.1 ships with more than fifty choices ranging from introductory and
standard matches to Napoleon's 1805 campaign, ancient Greece, exploration,
science fiction and Second World War scenarios. Each module can define its own
terrain, units, economy, sides and victory conditions.

The Macintosh interface presents the battlefield as a scrolling hex map. Select
units with the mouse, inspect terrain and objectives in the side panels, and use
the menus and keyboard commands to issue orders. Multiplayer support is included,
while the bundled scenarios also support local computer-controlled opponents.

## Preserved with its source

This catalogue entry uses the complete, unchanged Xconq 7.0.1 Macintosh
distribution. The matching [complete source code](https://ibiblio.org/pub/Linux/games/multiplayer/xconq-7.0.1.src.tgz)
is provided alongside it under GNU GPL version 2.

Systemless was tested beyond startup: a deterministic run opened the scenario
picker, selected the Austrian campaign of 1805, entered the live hex map and
processed map input. That test also uncovered a generic hidden-dialog visibility
bug in Systemless; the resulting regression fix now keeps Xconq's scenario list,
artwork and description panes visible when the game window is shown.

![Xconq gameplay](https://assets.systemless.org/catalogue/media/sha256/7d/7de2e09ab4d9f0aac3423f19db43b45a9b27930ff69d6bcc4a2c5bac12d2a4ad.png)
