---
id: tarantula
kind: game
title: Tarantula 1.0
summary: >-
  Build suit sequences from king to ace across ten columns in Andrew Warinner's
  two-deck Spider solitaire.
developer: Andrew Warinner
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: ce44cad5 deterministic play runner
    architecture: 68k
    environment: >-
      The original Info-Mac archive ran natively in headless mode. After OK on the
      unregistered notice, the two-deck tableau was dealt into ten columns.
      Clicking the stock dealt a new face-up row onto every column. A matched
      control without the click was identical until the input and then differed by
      21,634 pixels. An exact replay matched both checkpoints, with zero exhausted
      frames. Card dragging, completed suits, scoring and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5445
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: ce44cad5 browser build with launch flag
    architecture: 68k
    environment: >-
      Three release-mode headless Chromium 156 (aarch64 Linux) worker/WebGL runs
      of a locally served trunk build used the exact pinned original archive.
      Scripted ordinary clicks dismissed the unregistered notice and dealt from
      the stock. Twenty-second samples reached 59.97, 60.00 and 60.00 guest
      ticks per second, with no console errors. This approves bounded browser
      launch; card dragging and completed suits remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5445
runtime:
  executable_path: Tarantula 1.0 folder/Tarantula 1.0
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://mirrors.nic.funet.fi/pub/mac/info-mac/game/crd/tarantula-100.hqx
    expected_sha256: 2dec515826f48b5cd399104acf8b6b1e11ca8f5db6c2ae2cae1e1b80c8e53534
    expected_size: 508574
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/crd/tarantula-100.hqx
    - https://github.com/benletchford/systemless/issues/5445
    license: Original shareware distribution notice
    rights_holder: Andrew Warinner
    permission: >-
      The bundled Read Me permits freely distributing unregistered copies, as long as
      Tarantula and all its accompanying files are distributed together. Charging for
      distribution through an online service or on storage media needs the author's
      prior written consent. Preserve the complete unchanged archive.
    notes: US$10 shareware. The archive contains the 68K application, notes and Read Me.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/tarantula/tableau.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5445
    permission: Fresh gameplay capture from the permitted unchanged original package.
    notes: Native 68K tableau after dealing a row from the stock, cropped to the game window.
references:
- https://github.com/benletchford/systemless/issues/5445
---

## Spin your web

![Tarantula tableau](incoming/tarantula/tableau.png)

Two decks are dealt into ten columns. Build down regardless of suit, and remove
complete runs from king to ace of a single suit. When you're stuck, deal another
row from the stock.

Click **OK** on the unregistered notice. Drag cards to move them, and click the
stock in the top-left corner to deal.
