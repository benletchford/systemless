---
id: spacestation-pheta
kind: game
title: Spacestation Pheta
summary: Walk and climb through a space station to collect its key and reach the exit.
developer: Tad Woods / T&T Software
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 0f24e29c473c5ed912fa7cca50be45e38698ae11
    architecture: 68k
    environment: >-
      Complete original unregistered 2.7 archive, Spacestation Pheta executable,
      68K/depth 8. Original About visibly says UNREGISTERED. Not Yet and Pheta / New
      Game reach Screen 1; ordinary L walks the player right. Eight measured pixel
      assertions pass at 860 frontend / 1438 guest ticks with zero exhaustion;
      captures inspected. All five original files, ten forks and supported Finder
      metadata match production packing. Browser remains pending. Options initially
      has blank keyboard labels, Quick Playing Instructions is blank, and closing
      Options damages the playfield. Broader progression, jumping, saving and audio
      remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4628
runtime:
  executable_path: Spacestation Pheta 2.7/Spacestation Pheta
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://www.gryphel.com/d/sw/games/pheta/c/spacestation_pheta_2.7.hqx
    expected_sha256: a8b39aed0d8aed871c79d4297dd19d37d4a4eaeeb0884ab872da9f9c1b8cfc54
    expected_size: 281913
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/pheta/index.html
    - https://github.com/benletchford/systemless/issues/4628
    license: Original conditional shareware-library permission
    rights_holder: Tad Woods / T&T Software
    permission: >-
      Original Read Me - Pheta Release Notes expressly permits inclusion in shareware
      libraries provided About Pheta indicates Status UNREGISTERED and the package
      includes Pheta Main Screens and the release notes. Both conditions are verified;
      retain the complete unchanged original archive, its notices and shareware restrictions.
    notes: >-
      Five original files: Spacestation Pheta, Pheta Main Screens, Read Me - Pheta
      Release Notes, 54 New Dalton Screens and Big Cheese Screens. Independent
      unar matches all ten forks and supported Finder metadata against production
      packing. Application data fork is empty; resource fork is 250099 bytes,
      with seven CODE resources and no cfrg/PPC slice. PowerMac compatibility in
      the ReadMe does not establish a native PPC executable. No code, registration
      state, screens, feature restrictions or package contents were modified.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/spacestation-pheta/screenshot.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4628
    license: Original conditional shareware-library permission
    rights_holder: Tad Woods / T&T Software
    permission: Original game content accompanying the complete permitted unregistered shareware package.
    notes: >-
      Actual original Screen 1 after walking right, cropped to game content and
      its score/oxygen strip; no window title, frame, menu bar or desktop.
references:
- https://www.gryphel.com/c/sw/games/pheta/index.html
- https://github.com/benletchford/systemless/issues/4628
---

Play the original **Spacestation Pheta 2.7** unregistered shareware edition by
Tad Woods. The complete original package includes its main screens, release
notes and two additional screen files. Registration-only features retain their
original restrictions.

Choose **Not Yet**, then **Pheta → New Game**. **L** walks right using the original
keyboard assignment. First-screen native movement is verified; browser approval
is pending.

Game Options initially has blank keyboard labels, Quick Playing Instructions
is blank, and closing Options can damage the playfield. Broader progression,
jumping, saving and audio remain unverified.

![Spacestation Pheta Screen 1 after walking right](incoming/spacestation-pheta/screenshot.png)
