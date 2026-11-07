---
id: space-joust
kind: game
title: Space Joust 2.0.6
summary: Thrust, turn and fire in a two-ship space-combat arena.
developer: John M. Dole
publisher: Mind-ware
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original, explicit executable and 68K, 800x600/depth 8.
      Ordinary Not Yet and Play reach the two-ship arena. Default Speedster/Saucer,
      both AI 0 and physics settings remain intact. Original Controls lists
      Option/Command turning, Control thrust and Shift shooting. Held Control/Shift
      moves Speedster and emits a green projectile. Equal-clock no-fire control
      retains thrust and omits only Shift, moving without the projectile. Fresh
      active/control repeats each pass twelve measured pixels plus five exact prior
      own-run captures, seventeen checks at 648 frontend/1248 guest ticks, zero
      exhaustion. Own-run consistency is not hardware-oracle accuracy. Native binary
      SHA-256 a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0;
      Rust/Cargo runtime matches current master 73c9c1ce. The Controls dialog has
      overlapping Radio Button labels; rendering fidelity needs review. Browser
      input/pacing and released public play remain unverified. Launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2312
runtime:
  executable_path: Space Joust 2.0/Space Joust 2.0
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/space-joust/original.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Space%20Joust%202.0.sit
    - https://github.com/benletchford/systemless/issues/2312
    rights_holder: John M. Dole / Mind-ware
    permission: "Retained Space Joust Manual permits non-profit distribution without prior written notice only when unmodified and complete; profit distribution requires written permission. Preserve the full original archive, manual, story, blueprints, ship files, backgrounds, data, sprites, sounds and icon, John M. Dole and Mind-ware credits, original thirty-day shareware reminder and time limit. No registration code, date reset or paid-feature patch."
    license: Original complete-package non-profit distribution grant
    notes: "All retained data/resource notices reviewed. Original 852613 bytes, SHA-256 c3ff69bde46cd5147939565d4143341669b8bba6db845040711ce92c0c50070e. Independent extraction matches seventeen paths, thirty-four forks and seventeen supported Finder records, strict pack end 2055846. Actual resource-only executable has eight CODE resources and no cfrg/native PPC slice. Retained story and version labels identify 2.0.6 despite archive/application filenames saying 2.0. Actual startup/menu show an ordinary unregistered shareware copy, not the generic unused test-version banner retained among resources. This is a fresh replay after the old quality rejection, not reliance on its prior approval."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/space-joust/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2312
    rights_holder: John M. Dole / Mind-ware
    permission: "Original gameplay capture from the permitted unchanged distribution."
    notes: "Exact 640x480 content crop at (80,60), excluding surrounding letterboxing, menus and host UI; no resampling or editing. Speedster and its green projectile are visible. Not an independent hardware reference."
references:
- https://github.com/benletchford/systemless/issues/2312
---

![Space Joust gameplay](incoming/space-joust/gameplay.png)

Choose **Not Yet**, then **Play**. In the original default controls, use **Control**
to thrust, **Option/Command** to turn and **Shift** to fire. The original thirty-day
shareware trial and time limit remain intact.

Native movement/firing and an omitted-fire control are verified. The Controls
dialog has overlapping labels. Browser input/pacing, rendering fidelity, hits,
scoring, full rounds, two-player controls, custom ships/backgrounds, trial-limit
traversal, persistence and sound fidelity remain unverified. Launch stays disabled.
