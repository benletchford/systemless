---
id: chiral
kind: game
title: Chiral
summary: Join coloured atoms into molecules in Ambrosia's chemistry puzzle game.
developer: Andrew Welch
publisher: Ambrosia Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 26233b9b0ed9d5c9a13d270d5b7b2c2995cabe36
    architecture: 68k
    environment: >-
      Fresh optimized release-mode Chrome preview, normal WebAssembly worker at
      25 MHz. Original Not Yet dismissal, Play Chiral and direct atom placement
      work with stationary pointer down/up. A clean reload repeats startup and
      placement with brief back-to-back down/up commands, no drag or runtime patch.
      Preview requests supplied independently hash-verified original bytes.
      Full levels, scoring, sound and saves remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4393
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: "0.83.0"
    architecture: 68k
    environment: Native headless replay of the unchanged Chiral 1.0.0 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: "0.84.1"
    architecture: 68k
    environment: >-
      Deployed browser WebAssembly benchmark fetching the unchanged Systemless-hosted
      archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: b4c289ac4e8ee67836281d94ce54a9a724a49802106adb49a57741c5570b81d3
    size_bytes: 1043239
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/c/
    - https://www.vintageapplemac.com/files/games/Chiral%201.0.0%20%C6%92.sit
    license: Ambrosia Software nonprofit distribution licence
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      The bundled Chiral License permits nonprofit distribution without prior written
      notice when the software is unchanged and the complete works are included.
      Distribution for profit requires written permission.
    notes: >-
      Complete ten-file original archive. The licence text states unlimited free use,
      while the game displays a 30-day shareware notice; the notice and all original
      files remain intact. No registration code or bypass is supplied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 6e3e3ac484e3a0b9c2dd13bb9b8164aa3e3930eb4206c5e3906c22391e4fbb6c
    size_bytes: 518182
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4353
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Exact original hosted archive, 68K route, deployed browser WebAssembly 0.84.1.
      Level 1 accepts placement of a purple atom. The game fills the entire
      framebuffer; no desktop, menu bar or emulator framing is present.
references:
- https://www.vintageapplemac.com/software/games/c/
---

![Chiral gameplay](https://assets.systemless.org/catalogue/media/sha256/6e/6e3e3ac484e3a0b9c2dd13bb9b8164aa3e3930eb4206c5e3906c22391e4fbb6c.png)

Build molecules by placing coloured atoms on the board. Level 1 asks for two
molecules containing at least six atoms each.

Choose **Not Yet** at the original registration notice, then **Play Chiral**.
Keep the pointer inside the button until its action completes. Fresh optimized
release-preview checks accept stationary clicks without a drag.
Click the playfield to place the atom currently shown in the dispenser.

This entry uses the complete original **1.0.0** archive. Bounded native and
browser testing covers starting Level 1 and placing an atom. The deployed
browser benchmark fetches the exact Systemless-hosted archive; a separate
optimized release-player preview repeats startup and atom placement with brief
stationary clicks after a clean reload. Level completion, scoring, audio,
sustained play and save persistence remain unverified. The earlier stale preview
click symptom remains recorded separately; no new emulator fix is claimed.
