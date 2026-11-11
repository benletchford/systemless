---
id: simon
kind: game
title: Simon
summary: >-
  Watch the colored lights and repeat their growing sequence in Fernando Lagos's
  recreation of the electronic memory game.
developer: Fernando Lagos
year: 1997
architectures:
- 68k
- ppc
default_architecture: ppc
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 331fde2bec54470cc1151060e934dcb7121c296c native synchronous-sound candidate
    architecture: ppc
    environment: >-
      Complete unchanged original archive and exact executable, 25 MHz, 800x600/depth
      8, original SIZE and difficulty. Play displays the first green light for the
      duration of its synchronous sound. After it goes dark, clicking green repeats it
      correctly and advances to a green-blue sequence; the equal-clock omitted-response
      control does not advance and later repeats the initial green. Active and control
      each pass eight measured pixel checks and seven exact prior own-run image
      comparisons, frontend/guest 488 ticks and zero exhausted frames. Actual captures
      inspected; own-run consistency is not a hardware oracle. Binary SHA-256
      a585644bc7bd9362ce2aeff462034475160815a96639ef206e3bfbe88425cd03. Requires the synchronous PowerPC
      SndPlay fix in PR 5819. Full levels, best-score entry, persistence, audio
      fidelity, 68K gameplay, browser pacing and public release remain unverified. Launch
      stays disabled pending the runtime fix and browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5817
runtime:
  executable_path: Simon 1.0
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 16661d462cc7ac2b2597c8c2b208c8dc3a7cf3c55bb1eea33afd26753b858ef5
    size_bytes: 229837
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/simon-10.hqx
    - https://github.com/benletchford/systemless/issues/5817
    license: Author-permitted unchanged free noncommercial distribution
    rights_holder: Fernando Lagos; retained third-party notices
    permission: >-
      The complete Simon US README explicitly permits free FTP or bulletin-board
      distribution with its accompanying documentation. Commercial distribution and
      charging for the software or documentation are forbidden. Preserve the original archive,
      all notices and both original CPU slices.
    notes: >-
      Full 1966-byte Readme reviewed. It retains the SIMON trademark notice for
      TopToys and credits Jean-Francois Burnol for the WDEFs. Independent extraction matches
      both original files, all four forks and both Finder records. The same-named
      directory/application requires excluding directory index zero during independent unar
      extraction, without changing the archive. Production KPK is 346120 bytes,
      SHA-256 9e76266905bb128886dfb477af0ed2152b70ef2908b966834c92389ff5ef0bd0.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: fb05e340e1487a946391a314e90f21dd0f237bc7f63f45d284c0cb6ca92ae0ce
    size_bytes: 29211
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5817
    permission: Gameplay capture from the unchanged freely distributed original.
    notes: >-
      The blue light in the second sequence after correctly repeating the first
      green. Native 800x600 capture cropped to (216,123)-(606,513), 390x390, without
      resampling; excludes the Mac menu bar and host interface.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/simon-10.hqx
- https://github.com/benletchford/systemless/issues/5817
---

## Follow the lights

![The blue light in Simon's second sequence](https://assets.systemless.org/catalogue/media/sha256/fb/fb05e340e1487a946391a314e90f21dd0f237bc7f63f45d284c0cb6ca92ae0ce.png)

Press Play, watch the colored lights, then click them in the same order. Each
successful repetition adds another color. Last repeats the previous game, and
Scores shows the best scores. The original Readme describes four difficulty
levels requiring sequences of 8, 16, 32 or 64 strokes.
