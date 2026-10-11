---
id: on-the-edge
kind: game
title: On the Edge
summary: Place face cards around the edge and clear number cards totalling ten.
developer: John Stiles
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 392e612c55387756ec217577f085b387948da491
    architecture: 68k
    environment: >-
      Optimized native replay of the complete unchanged On the Edge 3.1 BinHex/StuffIt
      archive, selector On the Edge 3.1/On the Edge, explicit 68K and depth 8.
      Ordinary Command-N starts a new game. Clicking the inner square places the
      dealt six of hearts, draws the next ace of clubs and changes Score 0 to 100.
      Fresh replay passes twelve measured before/after pixel checks at 786 frontend
      / 1386 guest ticks with zero exhausted frames; captures inspected.
      Both files, all four original forks and Finder type/creator/flags match
      independent unar expansion. This proves one placement only. Full matches,
      card removal, undo, saving, audio, hosted browser input and public replay
      remain unverified. Launch stays disabled pending browser approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4661
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 400a685b6365aa226cb220bf7b8da61d4600bbbb
    architecture: ppc
    environment: >-
      Explicit native PPC probe of the identical original archive halts at frontend
      tick 1 / guest tick 0 before its requested startup checkpoint. PC and LR are
      zero, SP 07F6FFC0, 41206 cycles; only an empty framed window appears.
      Original register and memory-write traces identify zero-extended negative
      Random results producing negative shuffle indexes, overwriting the quit flag.
      The signed-result fix and exact original-game retest remain pending.
      Actual CODE 0/1/3 and a pwpc PEF/cfrg confirm both game slices; slice presence
      does not establish PPC playability.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/4663
runtime:
  executable_path: On the Edge 3.1/On the Edge
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://www.gryphel.com/d/sw/games/edge/c/ontheedge.sit.hqx
    download_page: https://www.gryphel.com/c/sw/games/edge/
    expected_sha256: 666d412e535e9be3d4b994be9ef1e04ec43ed77fa8a0820d7a877a9a4e80bde3
    expected_size: 22676
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/edge/
    - https://github.com/benletchford/systemless/issues/4661
    license: Original free-distribution shareware grant
    rights_holder: John Stiles
    permission: >-
      The original accompanying PICT readme expressly permits free copying and
      distribution when no charge is made. Sold disk/CD software libraries,
      for-profit collections and commercial bundling require written author consent.
      This free preservation distribution retains the complete original archive,
      accompanying readme, copyright notice and five-dollar shareware request.
      No sale, commercial-bundling or modified-software permission is inferred.
    notes: >-
      Original 22676-byte ontheedge.sit.hqx, not the repackaged HFS image.
      Production packing preserves both original files and all four forks exactly
      against independent unar extraction, including Finder type, creator and flags.
      Application: 15058-byte data fork / 21540-byte resource fork. PICT readme:
      2514-byte data fork / empty resource fork. Original vers identifies 3.1;
      preserved edition dates to 1995. Both 68K CODE and native PPC PEF/cfrg are
      present. No program, documentation, registration or archive bytes are altered.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/on-the-edge/card-placed.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4661
    permission: Systemless gameplay capture requested by the maintainer.
    notes: >-
      Content-only crop from the inspected native 68K assertion replay after one
      legal placement, score 100. Excludes desktop, menu bar and window title/frame.
      The original card-table artwork is retained; no filters or repainting.
---

![On the Edge after placing the six of hearts](incoming/on-the-edge/card-placed.png)

Use **File → New game** or **Command-N** to start. Place Kings, Queens and Jacks
in their labelled edge squares. Number cards may occupy the inner squares or
available edge squares. When the grid is full, remove combinations totalling ten.
Fill every face-card position to win; drawing an unplaceable face card ends the game.

This is the complete original **On the Edge 3.1** shareware package. Its original
readme and five-dollar shareware request remain intact. The default edition uses 68K.
