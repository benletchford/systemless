---
id: absolute-solitaire
kind: game
title: Absolute Solitaire Demo
summary: Play a limited Klondike game from the 1996 Absolute Solitaire demonstration.
developer: Glenn Seemann
publisher: MacSoft
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-30"
    tester: Catalogue maintainer
    systemless_version: 0871d440 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo. Its splash advanced
      to a Klondike table. Two browser clicks on the stock changed the waste card from
      4 of spades to 10 of hearts, then to 5 of diamonds. The archive was fetched once.
      A 10-second active-table sample measured 60.0 host FPS, 60.1 guest ticks per
      second, maximum measured frame 13.8 ms, and minimum audio queue 131 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3585
  - date: "2026-09-30"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo at 800 by 600. Its splash advanced to a
      Klondike table; two clicks on the stock changed the waste card on the active board
      from 10 of clubs to 9 of diamonds.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3585
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: bc947326f58026703be05e9fa99b9ab7f3af5b0f3360b174eac61f97033a13e3
    size_bytes: 222937
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/absolute-solitaire
    rights_holder: Glenn Seemann / MacSoft
    permission: >-
      The unchanged archive contains only the Absolute Solitaire 1.0 Demo
      application, with no bundled redistribution restriction. The demo was distributed on
      contemporary magazine discs, including Inside Mac Games and MacAddict, as recorded by
      the source catalogue. No retail files are included.
    notes: >-
      Original 222,937-byte StuffIt archive, SHA-256
      bc947326f58026703be05e9fa99b9ab7f3af5b0f3360b174eac61f97033a13e3.
      The launch splash credits Glenn Seemann, Varcon Systems, and MacSoft.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: fdd440cdc6907bceb36faab9854b45836564d66410f8fe46d225da0393b7a14f
    size_bytes: 36314
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3585
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
    notes: >-
      599-by-350 browser capture of the active Klondike table after two stock draws.
      The crop excludes website framing, Mac desktop, menu bar, and window frame. PNG
      SHA-256 fdd440cdc6907bceb36faab9854b45836564d66410f8fe46d225da0393b7a14f, 36,314
      bytes.
references:
- https://classicmacdemos.com/absolute-solitaire
---

## Classic card play

![Absolute Solitaire Klondike table](https://assets.systemless.org/catalogue/media/sha256/fd/fdd440cdc6907bceb36faab9854b45836564d66410f8fe46d225da0393b7a14f.png)

The demo opens a Klondike board after its splash screen. Click the stock pile
to turn a card, then build the tableau and foundations using normal solitaire
rules.
