---
id: rogue
kind: game
title: Rogue
summary: Explore a changing dungeon, collect treasure and seek the Amulet of Yendor.
developer: Ken Arnold, Michael Toy, Glenn Wichman, Timothy Stoehr and Rick Holzgrafe
year: 1996
architectures: [68k]
default_architecture: 68k
category: Role-Playing
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: fbd12e1e33941e1da82b9ceb61ed28966571ed81
    architecture: 68k
    environment: >-
      Unchanged publisher Rogue III Mac release 6 archive, Rogue executable,
      68K/depth 8. Ordinary h moves one cell left in the original dungeon.
      Eight measured before/after pixel assertions pass at 662 frontend /
      1262 guest ticks with zero exhaustion; captures inspected. Independent
      unar matches all three original files, six forks and supported Finder
      metadata against production packing. Browser, broader progression,
      combat, saving and audio remain unverified.
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
    type: url
    url: https://www.gryphel.com/d/sw/games/rogue/c/Rogue.sea.hqx
    expected_sha256: 9c1822d7ef48bd9de01cc186b0280df6f993d109de066c676fb7a5a4f206d173
    expected_size: 127671
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
      Original Rogue Manual explicitly permits redistribution in source and
      binary form subject to retained copyright, conditions and disclaimer,
      Berkeley acknowledgement in advertising materials, and no endorsement
      by the University or contributors without specific permission. Preserve
      the complete unchanged package, original manual and Macintosh port README.
    notes: >-
      Publisher archive is byte-identical to the original-format preservation
      copy; MD5 a4d26a1586c4fa7685092b1fe32c45d4 matches its published receipt,
      signature not independently verified. Three original files are Rogue,
      Rogue Manual and Read Me First. Game data fork is empty; resource fork
      is 108691 bytes, with four CODE resources and no cfrg/PPC slice. The
      Macintosh port is by Rick Holzgrafe, based on Timothy Stoehr's portable
      Rogue derived from Rogue 5.3. All six forks and Finder metadata match.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/rogue/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4612
    license: BSD-4-Clause
    rights_holder: The Regents of the University of California and contributors
    permission: Original game content under the retained bundled BSD conditions.
    notes: Actual original dungeon after moving left; cropped to game content without window frame, menu bar or desktop.
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

Native first-dungeon movement is verified. Browser qualification, broader
progression, combat, saving and audio remain pending.

![Rogue dungeon after moving left](incoming/rogue/gameplay.png)
