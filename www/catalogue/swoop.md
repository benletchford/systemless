---
id: swoop
kind: game
title: Swoop
summary: Defend your ship against waves of rendered aliens in Ambrosia's arcade shooter.
developer: David Wareing
publisher: Ambrosia Software
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 4d2c64b90585aed2f08bd193ff4ae6214a769658
    architecture: 68k
    environment: >-
      Native bounded frontend-tick replay from the exact original 1.0.2 archive,
      ordinary executable selection after Smaller Installer expansion. Original Not Yet, N
      to start, X for movement and period for firing. Inspected active wave and shot; a
      repeat passes four measured pixel assertions and six exact screenshot
      comparisons, with no instruction-budget exhaustion. Original installed forks independently
      verified by running the bundled installer. Browser gameplay, full waves, scoring,
      sustained play, saves and audio unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4398
runtime:
  runtime_pacing:
    cpu_mhz: 8
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 53dbe824bc2a21c4b6a6b1e2c510a01d6d700f67d3401f66a1ab8cc4d98a70d5
    size_bytes: 2739051
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/s/
    - https://www.vintageapplemac.com/files/games/Swoop%201.0.2.sit
    - >-
      https://github.com/benletchford/systemless/releases/tag/catalogue-intake-20261010
    license: Ambrosia Software nonprofit distribution licence
    rights_holder: David Wareing and Ambrosia Software, Inc.
    permission: >-
      The original bundled Swoop License explicitly permits nonprofit distribution
      without prior written notice when the software is unchanged and the complete works
      are included. Distribution for profit requires written permission.
    notes: "Complete original 1.0.2 StuffIt archive containing its Smaller Installer. All twelve installed files are preserved: game, registration application, licence, release notes, FAQs, web reference, icon and all four media files. Original 30-day shareware trial retained; no registration code or bypass. The original FAQ states this version is not PowerPC-native."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 50d9d8ed6c98c67414f760003fd823a985038f212f0c526dcdf109716169abd6
    size_bytes: 7510
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4398
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Actual native 68K gameplay from the unchanged original archive, with a fired
      shot visible. Cropped to the game's centred 640-by-480 content surface; no emulator
      framing, host UI or classic Mac menu bar included.
references:
- https://www.vintageapplemac.com/software/games/s/
---

![Swoop gameplay](https://assets.systemless.org/catalogue/media/sha256/50/50d9d8ed6c98c67414f760003fd823a985038f212f0c526dcdf109716169abd6.png)

Fight waves of aliens in the original **Swoop 1.0.2** shareware release.
Choose **Not Yet** at the original trial notice, then press **N** to start.
Default controls are **Z** to move left, **X** to move right, **period** to fire
and **slash** to select a weapon. Press **C** at the menu to inspect or change
controls. **Escape** aborts a game; **Caps Lock** pauses it.

The complete original installer archive retains its licence, registration
application, documentation and media. Bounded native testing covers starting an
active wave, moving the ship and firing. This version uses 68K code; its original
FAQ describes Power Mac support through emulation. Browser approval and public
launch remain pending. Full waves, scoring, sustained play, saves and audio
remain unverified.
