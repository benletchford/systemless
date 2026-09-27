---
id: heaven-and-earth
kind: game
title: Heaven & Earth Demo
summary: Explore the Figure Ground tile puzzles in the original Macintosh demonstration.
developer: Software Resources International
publisher: Buena Vista Software
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 41c99f065a527b03598f9ee1c7adda9d77582cc0
    architecture: 68k
    environment: "Deterministic headless replay of the unchanged Macintosh demo with the PBOpenWD directory-path fix in PR #3034. Opened Illusion Gateway, entered Figure Ground, and moved a connected tile group on the puzzle board. BasiliskII showed a low-memory alert before gameplay in the native test environment, so native parity and browser launch are not yet verified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3027
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 14f7ee15a8e6596920ac6026017f7c8bbf9c5b506a61f2c338f937568f9fffc8
    size_bytes: 790390
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/heaven-and-earth
    - https://www.iangilman.com/software/heavenearth.php
    rights_holder: Heaven & Earth rights holders
    permission: >-
      This is the unchanged, purpose-built promotional Macintosh demo distributed by
      Classic Macintosh Game Demos, not the retail release. The archive has no express
      redistribution clause. The original creator also offers the classic Macintosh
      full version freely on his site; this entry uses only the demo.
    notes: >-
      Original 790,390-byte StuffIt archive, SHA-256
      14f7ee15a8e6596920ac6026017f7c8bbf9c5b506a61f2c338f937568f9fffc8. It contains the 68K Heaven & Earth DEMO
      application and its sibling H&E Resources directory.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0216c4d108de47318935c64cb8a46d9803230ad5995c7c5a30079683111e3623
    size_bytes: 23114
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3027
    permission: >-
      Fresh gameplay capture from the unchanged promotional demo for this catalogue
      entry. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 640-by-480 content crop at (80,80) of a deterministic Systemless
      framebuffer after moving a tile group in Figure Ground. Mac desktop, window chrome, and
      menu bar were excluded; no game pixels were changed. PNG SHA-256
      0216c4d108de47318935c64cb8a46d9803230ad5995c7c5a30079683111e3623, 23,114 bytes.
references:
- https://classicmacdemos.com/heaven-and-earth
- https://www.iangilman.com/software/heavenearth.php
- https://github.com/benletchford/systemless/pull/3034
---

## Figure Ground

![Figure Ground puzzle board after moving a connected tile group](https://assets.systemless.org/catalogue/media/sha256/02/0216c4d108de47318935c64cb8a46d9803230ad5995c7c5a30079683111e3623.png)

Heaven & Earth's Illusion Gateway presents visual puzzles with a quiet,
hand-drawn look. In Figure Ground, drag connected groups of patterned tiles
between two boards to recreate the target arrangement. The demo also offers a
noninteractive introduction to its other worlds.

This is the original 68K Macintosh demonstration, not the retail game. A
deterministic Systemless replay entered Figure Ground and moved a tile group.
Browser launch remains disabled pending manual testing and approval.
