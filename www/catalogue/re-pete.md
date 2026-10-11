---
id: re-pete
kind: game
title: Re-Pete
summary: Remember and repeat an increasing sequence of flashing buttons and tones.
developer: Tom Bender / Trans-Tex Software
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 1df6ea6dc2b5deab2f3466134b123ac308f13a70
    architecture: 68k
    environment: >-
      Unchanged publisher Classic 2.2 archive; explicit 68K, depth 8, complete
      original executable and embedded documentation. Native build includes the sound
      sequence/draining repair in PR 4657, now merged at d5b3864b. Ordinary Start Game
      displays the first button-5 flash across eight frontend captures. A measured note
      interval yields 2761 non-silent PCM samples. Matching input advances score 0 to
      1; the next displayed sequence is 5 then 7, and ordinary matching clicks advance
      score to 3. Sixteen measured pixel/audio checks pass across 530 frontend / 1102
      guest ticks with no budget exhaustion; captures inspected. Independent extraction
      matches both original forks and supported Finder metadata against production
      packing. This proves bounded native memory-game rounds, not all difficulty settings
      or independent audio fidelity. Promoted assets are hosted; browser preview
      evidence is recorded separately below and launch approval remains pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4657
  - date: "2026-10-12"
    tester: Catalogue maintainer
    systemless_version: 052693c3380a801374beca5374d62c8fa014cd01
    architecture: 68k
    environment: >-
      Production-origin browser preview with the promoted catalogue entry and rebased
      sound fix; local-only launch override excluded from the catalogue PR. Chrome
      151.0.7922.174, actual runtime worker, WebGL, isolated 800x600 canvas, depth 8,
      default 25 MHz. Only frontend files are substituted at the production origin;
      the archive request is untouched: HTTP 200, 73613 bytes and expected SHA-256.
      Ordinary retained-menu drag starts a round. The first button-5 flash is captured
      visibly golden, then clears. Ordinary matching clicks for rounds 5 and 5/7
      advance score 0 to 1 to 3. Twelve measured browser score-pixel checks pass in a
      fresh replay; captures inspected and pacing probe passes with no console errors.
      A preview-server reset interrupted the first asserted attempt before assertions;
      the unchanged retry passes. This is bounded browser preview qualification;
      all difficulty settings, browser audio fidelity, human release and independent
      public replay remain unverified. Current integrated-runtime replay and launch approval remain pending.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/4656
runtime:
  executable_path: Re-Pete 2.2/Re-Pete
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 8de6839796c93ba71a2a9f800a3e2d002f780f8e84356b717ef8c6e4e415add9
    size_bytes: 73613
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.tex-edit.com/index.html
    - https://www.gryphel.com/c/sw/games/repete/index.html
    - https://github.com/benletchford/systemless/issues/4647
    license: Publisher conditional unchanged-package redistribution permission
    rights_holder: Tom Bender / Trans-Tex Software
    permission: >-
      Publisher About Our Freeware permits distribution with documentation intact and
      programs unchanged, excluding beta reposts. This is its linked Classic release
      2.2, not a beta. Retain the complete original archive and embedded documentation
      without modification.
    notes: >-
      Publisher and preservation archives are byte-identical. One original file, two
      forks and supported Finder metadata independently match production packing. Empty
      data fork; 71628-byte resource fork with CODE 0/1/3 and no cfrg or native PPC
      slice. No program, resource, registration or documentation changes. The original
      embedded About dates version 2.2 to December 21, 1995. The separate OS X version is
      outside this classic entry.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 9f126808b4d5b80c37741d72ee20bfd06d335491b5d8d0f3c68174062c6561ac
    size_bytes: 6040
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://www.tex-edit.com/index.html
    - https://github.com/benletchford/systemless/pull/4657
    license: Publisher conditional unchanged-package redistribution permission
    rights_holder: Tom Bender / Trans-Tex Software
    permission: Original game content accompanying the complete permitted package.
    notes: >-
      Actual original game after two matching memory rounds, score 3; cropped to the
      game content without window frame, title, menu bar or desktop.
references:
- https://www.tex-edit.com/index.html
- https://www.gryphel.com/c/sw/games/repete/index.html
- https://github.com/benletchford/systemless/issues/4647
---

Play the original **Re-Pete 2.2** by Tom Bender. Choose **Game → Start Game**,
watch the flashing buttons and listen to their tones, then click the buttons in
the same order. Each new round adds another button to remember.

Two native and browser-preview memory rounds and their score progression are
verified. Launch remains disabled pending runtime integration and approval.
All difficulty settings and browser audio fidelity remain unverified.

![Re-Pete after two matching memory rounds](https://assets.systemless.org/catalogue/media/sha256/9f/9f126808b4d5b80c37741d72ee20bfd06d335491b5d8d0f3c68174062c6561ac.png)
