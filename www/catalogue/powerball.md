---
id: powerball
kind: game
title: Powerball 1.3
summary: >-
  Keep the ball in play and clear the bricks in this colorful Macintosh arcade
  game.
developer: Wolfgang Weber / Macnetic Software
year: 1994
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: CurMap and ResolveAliasFile candidates, later merged as a1e9f911 and 5b477c09
    architecture: 68k
    environment: >-
      The unchanged shareware archive reached the level-one board in a bounded native
      run at depth 8. A measured mouse movement moved the paddle while a matched
      no-movement control left it in place. Production packing independently matched all
      seven original files, their fourteen forks and supported Finder metadata. Full
      levels, saving and browser gameplay remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4681
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: f84f71d2502b129d82baca5ba41f6e2fff0ea577
    architecture: ppc
    environment: >-
      The same archive's PPC slice reached the level-one board at depth 8 with a
      measured paddle response and a no-movement control. The inspected candidate
      screenshot displays the score, paddle count, level and high score as 0, 5, 1 and 0,
      matching the 68K board; the text-mode correction is pending in PR 5381. Full levels,
      saving and browser gameplay remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4681
runtime:
  executable_path: Powerball Shareware/Powerball.fat
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 9bb06c85a6d31580f0842a508650cbcf34b0e3bf1013d2df810a9f902a2ddb3e
    size_bytes: 457888
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/p/
    - https://github.com/benletchford/systemless/issues/4681
    license: Original Powerball Manual Software License
    rights_holder: Wolfgang Weber / Macnetic Software
    permission: >-
      The original manual permits non-profit distribution without prior written
      notice when the software is unmodified and its complete contents are included.
      For-profit distribution requires signed written permission. Retain the original
      copyright notices and complete package. Registration is required after the original
      30-day evaluation period.
    notes: >-
      Complete original shareware archive, including the FAT application, manual,
      ReadMe and companion data and sound files. The unchanged archive is 457888 bytes
      with SHA-256 9bb06c85a6d31580f0842a508650cbcf34b0e3bf1013d2df810a9f902a2ddb3e.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b34d29611401cb1e2d180496e4229aec186d8b64db0c1cc20f4ee08511570e5f
    size_bytes: 16791
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4681
    permission: Original gameplay content captured from the complete permitted package.
    notes: >-
      Level-one PPC board from the unchanged original archive, cropped to its
      512-by-342 game surface without desktop or menu bar. The PPC text correction is pending
      in PR 5381.
references:
- https://www.vintageapplemac.com/software/games/p/
- https://github.com/benletchford/systemless/issues/4681
---

## Clear the bricks

![Powerball level-one board](https://assets.systemless.org/catalogue/media/sha256/b3/b34d29611401cb1e2d180496e4229aec186d8b64db0c1cc20f4ee08511570e5f.png)

Choose **Game → New** and move the paddle to keep the ball in play while clearing
bricks. This is the original shareware package; its manual describes a 30-day
trial and registration terms.

Native runs reached the first board on both included architectures. Browser
qualification is pending, so launch remains disabled.
