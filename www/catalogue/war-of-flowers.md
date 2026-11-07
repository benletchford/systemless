---
id: war-of-flowers
kind: game
title: War of Flowers 1.1
summary: >-
  Match flower cards and collect scoring combinations against two computer
  players.
developer: Minho Choi
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original, explicit executable and 68K, 800x600/depth 8.
      Ordinary title click, New War, first-player card and continue reach the board;
      original defaults remain virtual starting money 20, High computer IQ and Fast
      animation. Clicking the first hand card reduces the human hand from seven to six, changes
      the table/collections and receives computer turns. Equal-clock control preserves
      pointer movement but omits that final card click, retaining seven cards. Fresh
      active/control repeats each pass eight measured pixels and six exact own-run
      captures, fourteen checks at 1020 frontend/1620 guest ticks, zero exhaustion. Own-run
      consistency is not hardware-oracle accuracy. Native executable SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0; Rust/Cargo runtime matches
      merged 73c9c1ce. Multicolour table-background bands remain visible and rendering
      fidelity needs review. Browser input/pacing and released public play remain
      unverified. Launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2316
runtime:
  executable_path: War of Flowers/War of Flowers 1.1/War of Flowers
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 785223705c65ebd6c9972a8d6d66fe5fa8397a2f7d4ea454a65fab074bd34954
    size_bytes: 105469
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/War%20of%20Flowers.sit
    - https://github.com/benletchford/systemless/issues/2316
    license: Original complete-package online distribution grant
    rights_holder: Minho Choi
    permission: >-
      Original Read Me allows placement on online services charging only normal
      connect fees, prohibits profit distribution without written permission, requires
      accompanying documents, and forbids altered copies or altered card faces. Preserve the
      complete unchanged original package, Read Me, Release Note, icon and outer
      comment file, Minho Choi copyright and original fifteen-day shareware trial. No
      registration code, date reset or paid-feature alteration is used.
    notes: >-
      All retained data/resource notices reviewed; no exclusive-media clause found.
      Original 105469 bytes, SHA-256
      785223705c65ebd6c9972a8d6d66fe5fa8397a2f7d4ea454a65fab074bd34954. Independent extraction matches five paths, ten forks and five
      supported Finder records; strict pack end 370953. Actual resource-only game fork is
      343595 bytes, SHA-256
      35e43ac9d1114152fb9f039a3bcc1cd4251e211bc08055c11eb8e1db938c9485, CODE 0-3 with no cfrg/native PPC slice. Release Note dates version 1.1 to
      April 22, 1993.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8745b9dee604d5d831fdfe99463fb114a485cbfe425c2bc6cf2439005c3531a5
    size_bytes: 16640
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2316
    rights_holder: Minho Choi
    permission: >-
      Original gameplay capture from the permitted unchanged distribution; card
      artwork is unmodified.
    notes: >-
      Exact 630x440 board crop at (5,39), without resampling; excludes menu, window
      frame, desktop and host UI. Multicolour table-background bands are retained in the
      capture and remain a rendering concern; this is not an independent hardware
      reference.
references:
- https://github.com/benletchford/systemless/issues/2316
---

![War of Flowers gameplay](https://assets.systemless.org/catalogue/media/sha256/87/8745b9dee604d5d831fdfe99463fb114a485cbfe425c2bc6cf2439005c3531a5.png)

Click the title, choose **New War**, select a card to determine the first player,
then click to continue. On your turn, click a card in your hand; matching cards
are collected and the computers take their turns. The original online Help
explains scoring and calling GO or STOP.

The original fifteen-day shareware trial remains intact. Native card play and
an omitted-click control are verified. Table-background rendering, browser
input/pacing, scoring, complete wars, options, persistence and sound fidelity
remain unverified. Launch stays disabled.
