---
id: rogue
kind: game
title: Rogue
summary: Explore a changing dungeon, collect treasure and seek the Amulet of Yendor.
developer: Ken Arnold, Michael Toy, Glenn Wichman, Timothy Stoehr and Rick Holzgrafe
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: fbd12e1e33941e1da82b9ceb61ed28966571ed81
    architecture: 68k
    environment: >-
      Unchanged publisher Rogue III Mac release 6 archive, Rogue executable,
      68K/depth 8. Ordinary h moves one cell left in the original dungeon. Eight measured
      before/after pixel assertions pass at 662 frontend / 1262 guest ticks with zero
      exhaustion; captures inspected. Independent unar matches all three original files, six
      forks and supported Finder metadata against production packing. Browser, broader
      progression, combat, saving and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4612
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 8b9cf04b439f2fd3122935e62b70386fdf15a704
    architecture: 68k
    environment: >-
      Production-origin browser preview with only frontend assets supplied locally.
      Actual managed original archive returned HTTP 200, 127671 bytes and the expected
      SHA-256 without interception. Cross-origin isolation, an actual runtime worker
      and an 800x600 canvas were verified. Ordinary H moves the player one cell left;
      eight measured before/after pixel checks pass with the pointer outside the canvas.
      Captures inspected. Broader progression, combat, saving and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4612
runtime:
  executable_path: Rogue ƒ/Rogue
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 9c1822d7ef48bd9de01cc186b0280df6f993d109de066c676fb7a5a4f206d173
    size_bytes: 127671
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.semicolon.com/old/DownloadPage.html
    - https://www.gryphel.com/c/sw/games/rogue/index.html
    - https://github.com/benletchford/systemless/issues/4612
    license: BSD-4-Clause
    rights_holder: The Regents of the University of California and contributors
    permission: >-
      Original Rogue Manual explicitly permits redistribution in source and binary
      form subject to retained copyright, conditions and disclaimer, Berkeley
      acknowledgement in advertising materials, and no endorsement by the University or
      contributors without specific permission. Preserve the complete unchanged package, original
      manual and Macintosh port README.
    notes: >-
      Publisher archive is byte-identical to the original-format preservation copy;
      MD5 a4d26a1586c4fa7685092b1fe32c45d4 matches its published receipt, signature not
      independently verified. Three original files are Rogue, Rogue Manual and Read Me
      First. Game data fork is empty; resource fork is 108691 bytes, with four CODE
      resources and no cfrg/PPC slice. The Macintosh port is by Rick Holzgrafe, based on
      Timothy Stoehr's portable Rogue derived from Rogue 5.3. All six forks and Finder
      metadata match.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 6b0ef05acd158ba935d6ffc813aa426a79279173e80e9c6b4d48d3e0a6a06e5c
    size_bytes: 1587
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4612
    license: BSD-4-Clause
    rights_holder: The Regents of the University of California and contributors
    permission: Original game content under the retained bundled BSD conditions.
    notes: >-
      Actual original dungeon after moving left; cropped to game content without
      window frame, menu bar or desktop.
references:
- https://www.semicolon.com/old/DownloadPage.html
- https://www.gryphel.com/c/sw/games/rogue/index.html
- https://github.com/benletchford/systemless/issues/4612
---

Play the original **Rogue III Mac release 6**, ported to the Macintosh by Rick
Holzgrafe. The complete publisher archive includes the original manual and port
README. This edition contains a 68K game.

Use **H**, **J**, **K** and **L** to move left, down, up and right. Lowercase moves
one cell; uppercase continues until something stops you. See the bundled manual
for inventory, item use and other dungeon commands.

This product includes software developed by the University of California,
Berkeley and its contributors. The complete BSD conditions and disclaimer are
retained in the original Rogue Manual supplied with the game.

Native and browser first-dungeon movement are verified. Broader progression,
combat, saving and audio remain unverified.

![Rogue dungeon after moving left](https://assets.systemless.org/catalogue/media/sha256/6b/6b0ef05acd158ba935d6ffc813aa426a79279173e80e9c6b4d48d3e0a6a06e5c.png)
