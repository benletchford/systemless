---
id: super-mines-demo
kind: game
title: Super Mines Demo
summary: Sweep a lively minefield while managing a team of minesweepers.
developer: Callisto Corporation
publisher: Callisto Corporation
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 87205416 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo with the generic
      document-viewer selection fix. The demo splash opened instead of the bundled review,
      then the default picker reached the active minefield. A browser click on a board
      square changed its pixels. One exact archive fetch. A five-second active-board
      sample measured 60.0 host FPS and 60.2 guest ticks per second, maximum measured
      frame 3.4 ms, and minimum audio queue 131 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3609
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo at 800 by 600 using the nested game
      application, not its bundled review document. Dismissed the splash, accepted the default
      game picker, and reached the live minefield. A click on a covered square
      revealed a mine on the board.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3609
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 73902066c74800d7fbc7550572284adf86612d9dd8a8b69ea59423444b39345e
    size_bytes: 282312
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/super-mines
    - https://www.keystonemac.com/pdfs/Catalog_Jul05.pdf
    rights_holder: Callisto Corporation
    permission: >-
      This archive contains the purpose-built Super Mines Demo application and a
      contemporary review, not the retail game. A Macintosh demo index independently lists
      Super Mines Demo among game demos. The bundled material contains no additional
      redistribution restriction.
    notes: >-
      Unchanged 282,312-byte StuffIt archive, SHA-256
      73902066c74800d7fbc7550572284adf86612d9dd8a8b69ea59423444b39345e.
      The promoted public asset was fetched back and matched this hash.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8b70cff89497d94b8a2561fe9af9311f5cb9e8b2e31f0b9b82c2f56ca19b6c27
    size_bytes: 6343
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3609
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      489-by-287 direct Chrome capture of the active minefield after a click. PNG
      SHA-256 8b70cff89497d94b8a2561fe9af9311f5cb9e8b2e31f0b9b82c2f56ca19b6c27, 6,343
      bytes.
      The promoted public asset was fetched back and matched this hash.
references:
- https://classicmacdemos.com/super-mines
- https://www.keystonemac.com/pdfs/Catalog_Jul05.pdf
---

## Clear the field

![Super Mines minefield](https://assets.systemless.org/catalogue/media/sha256/8b/8b70cff89497d94b8a2561fe9af9311f5cb9e8b2e31f0b9b82c2f56ca19b6c27.png)

Click the demo splash, accept the default game and difficulty, then uncover
covered squares while watching the minesweeper count and fuse.
