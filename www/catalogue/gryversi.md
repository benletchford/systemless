---
id: gryversi
kind: game
title: Gryversi
summary: Play Reversi against the computer by enclosing and flipping its discs.
developer: Paul C. Pratt
year: 2006
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: 25bd439d950f84431d2d984eca5242a22b31ad49
    architecture: 68k
    environment: >-
      Unchanged author-hosted 1.0.1c ZIP and HFS image, Gryversi executable, native
      68K and depth 8. Ordinary Time menu selects 1 Second. White places a legal disc
      and flips a Black disc; the computer replies and flips another White disc to Black.
      Eight measured before/after pixel assertions pass at 754 frontend / 1334 guest
      ticks with zero exhausted frames; captures inspected. Independent extraction
      verifies all six original HFS files and twelve forks, supported Finder metadata and
      the ZIP checksum receipt against the production pack. Full matches, save/load,
      audio, HyperCard documentation rendering and browser launch qualification remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4584
  - date: "2026-10-11"
    tester: Catalogue maintainer
    systemless_version: e30e20a2d7824c9318566beb6e317f31e4ecf512
    architecture: 68k
    environment: >-
      Optimized Chrome preview from the promoted revision with its local launch
      switch enabled. Controlled local frontend responses under the production
      origin; actual Systemless managed archive fetch untouched, HTTP 200,
      original SHA-256 and 45947-byte body verified. Isolated 800x600 worker/WebGL,
      25 MHz and maximum two ticks per paint. Ordinary Time menu selects 1 Second;
      a legal White placement flips a Black disc and the computer replies with
      another disc flip. Eight measured before/after pixel checks pass; captures
      inspected. Complete matches, saving, audio, HyperCard documentation rendering
      and post-release public replay remain unverified. Browser launch approved.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4584
  - date: "2026-10-13"
    tester: Catalogue maintainer
    systemless_version: v0.97.0
    architecture: 68k
    environment: >-
      Direct public Chrome Worker/Canvas2D replay on systemless.org/gryversi/,
      default 25 MHz and configured depth 8. Actual deployed app and managed original
      ZIP, with no preview frontend or local archive fixture. Normal Time, 1 Second;
      White plays zero-based row 2, column 4 and flips Black at row 3, column 4.
      Computer Black replies at row 4, column 5 and flips row 4, column 4. All eight
      fixed before/after pixel checks pass; raw game captures inspected. Pacing is
      58.438 guest ticks/sec above the unchanged minimum 50, with no console errors.
      Fresh public ZIP GET is 45947 bytes with the exact original SHA-256. Published
      v0.97.0 deployment and all six native package jobs succeed. Colour references
      are inspected game pixels and the documented native move, not an independent
      Macintosh emulator oracle. Full matches, saving, audio and HyperCard
      documentation rendering remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4584
runtime:
  executable_path: Gryversi/Gryversi
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: 54e31dd314cce3b669247af6533e2972994f29cb49b78c7f54572c950eb1f44e
    size_bytes: 45947
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/c/sw/games/gryversi/index.html
    - https://github.com/benletchford/systemless/issues/4584
    license: GPL-2.0-only
    rights_holder: Paul C. Pratt
    permission: >-
      The author's page and original README expressly allow redistribution under GNU
      GPL version 2. Preserve the entire unchanged package, including original app.c
      source, COPYING licence, README, game resources and documentation. Source is
      supplied inside the same managed download as the game.
    notes: "Publisher MD5 e9baccabcb96a8f8a7f8cbd43f7b2e1d matches; signature not independently verified. Embedded HFS image is 819200 bytes and its MD5 c7b9e146a749b91198afafab3a46adac matches the bundled receipt. Original game has an empty data fork and CODE 0 (24 bytes), CODE 1 (15996 bytes), no cfrg: this edition has a genuine 68K game and no native PPC slice. Independent extraction matches all original fork bytes and Finder type/creator/flags; original HFS volume metadata fields match, with guest volume named Gryversi rather than original Untitled. HyperCard Player is not included."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ea34d253b02f4524ad9510f7fe36cb9a9f0d1f32a8ed4ba9e4c8e5aec24cc26b
    size_bytes: 558
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4584
    license: GPL-2.0-only
    rights_holder: Paul C. Pratt
    permission: >-
      Actual Systemless capture of the unchanged GPL game. Original game artwork is
      covered by the author's whole-package GPL version 2 distribution grant.
    notes: >-
      Captured after a legal White move and computer Black reply; cropped to the
      complete eight-by-eight grid. Excludes menu bar, window frame and host UI.
references:
- https://www.gryphel.com/c/sw/games/gryversi/index.html
- https://github.com/benletchford/systemless/issues/4584
---

Place a disc on an empty square that encloses opposing discs between your new
disc and one of your existing discs. Enclosed discs flip to your colour.
White moves first in this edition. The computer plays Black by default.
Use the Time menu to choose how long the computer thinks before moving.

![Gryversi after a White move and the computer's Black reply](https://assets.systemless.org/catalogue/media/sha256/ea/ea34d253b02f4524ad9510f7fe36cb9a9f0d1f32a8ed4ba9e4c8e5aec24cc26b.png)

This original release is free software under GNU GPL version 2. Its unchanged
ZIP includes the original C source, licence and documentation with the game.
The documentation is a HyperCard stack; a HyperCard player is not bundled.

Native and browser placement, flipping and a computer reply are verified.
Complete matches, saving and audio remain unverified.
