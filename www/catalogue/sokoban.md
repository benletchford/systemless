---
id: sokoban
kind: game
title: Sokoban 2.4.2a
summary: Push treasure chests onto their destinations in Scott Lindhurst's warehouse puzzle.
developer: Scott Lindhurst
year: 1999
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged 2.4.2a archive, exact game selector, explicit 68K and
      800x600/depth 8. Return advances a startup notice with a blank body to the new-player
      dialog. Ordinary typing creates Player. Up and five Left moves approach a chest;
      a final Left pushes it one cell, changing Moves 6/Pushes 0 to Moves 7/Pushes 1.
      The equal-duration control omits only the last input and leaves the chest in place.
      Fresh active/control repeats each pass six measured pixels and six exact prior
      Systemless captures, 12 checks at 437 frontend / 1037 guest ticks, zero exhaustion.
      Repeated captures are not hardware oracle evidence. Native binary SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0.
      Production packing independently matches all twelve original paths, twenty-four
      forks and twelve supported Finder records against unar, with strict end validation.
      Runtime source is identical to merged 2a0e56f6. Browser input/pacing, completed
      levels, startup-notice fidelity, persistence and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5711
runtime:
  executable_path: Sokoban 2.4.2a/Sokoban 2.4.2a
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Sokoban%202.4.2a.sit
    expected_sha256: 8070a88266b8af34304cf1799a8ee9b873c0f56571074480c72e49d798bb352c
    expected_size: 144881
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Sokoban%202.4.2a.sit
    - https://github.com/benletchford/systemless/issues/5711
    license: Original online-distribution grant
    rights_holder: Scott Lindhurst
    permission: >-
      Original Sokoban docs explicitly permit online distribution by online services or
      BBSs provided this documentation remains included unchanged. The retained About
      artwork independently permits distribution via online services. CD-ROM distribution
      has a separate reference-copy condition. Preserve the complete original package,
      documentation and notices, including the authorized twenty-level Sokowahn free
      sample and original paid-level ordering conditions. No paid levels are unlocked.
    notes: >-
      Original 23877-byte docs SHA-256 b9c1e61f1cfdd0ae838a5881a25d0994459411add6fe88b9939804b9f978a3a3.
      All retained data/resource notices were reviewed before promotion. Version 2.4.2
      documents removal of disputed Boxxle and IQ Carrier levels. Actual game resource fork
      is 139567 bytes, SHA-256 5d78bd692a739a3870e15196780511d5b284d0698f1e3a87019cd3154c4842cc,
      with CODE 0-4 and no cfrg: 68K, independently of the bundled font file.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/sokoban/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5711
    permission: Original gameplay capture from the permitted unchanged distribution.
    notes: >-
      Native 357f79d1 after a legal chest push. Pixel-exact 496x300 content/HUD crop
      at (151,90), without resampling; excludes menu bar, desktop and window framing.
references:
- https://github.com/benletchford/systemless/issues/5711
---

![Sokoban after a legal chest push](incoming/sokoban/gameplay.png)

Press **Return** at startup, enter a player name and accept. Use arrow keys or
click adjacent squares to move. Push each treasure chest onto a destination;
chests cannot be pulled. **Escape** undoes a move; **Space** redoes it.

The bounded native replay verifies movement and a chest push against an
omitted-input control. Browser qualification, completed levels, startup-notice
fidelity, saved progress and audio remain unverified. Launch stays disabled
pending browser approval.
