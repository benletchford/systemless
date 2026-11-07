---
id: boka-battleships
kind: game
title: Boka Battleships 1.0
summary: >-
  Arrange your fleet and search the opponent's grid in this naval strategy game.
developer: Marc Boxall
year: 1999
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-06"
    tester: Catalogue maintainer
    systemless_version: 7bd49954b4db0d4036496a4ae5ec7ca8520aa473 private popup-menu candidate
    architecture: 68k
    environment: >-
      Native 68K, depth 8, candidate based on 584f6e86 with popup repair PR 5702.
      Exact original archive and game selector; production decoding matches all three
      installed paths, six forks and three supported Finder records against independent
      unar extraction, with strict pack EOF. Select Little Johnnie, move a ship down one
      cell, confirm placement and fire a miss; the computer replies with a miss.
      Active and no-shot repeats each pass five pixel assertions and nine exact captures
      at 801 frontend / 1398 guest ticks. No-drag control passes three pixel assertions
      and six captures at 555 / 1155; no-Done control passes five assertions and nine
      captures at 801 / 1401 and remains in placement. All have zero exhaustion.
      These are same-runtime repeat and input-omission controls, not independent hardware
      parity. Original trial and registered-only restrictions remain intact. Requires the
      unmerged popup fix; browser pacing and released/live behavior are unverified.
      Victory, saving, two-human play and audio fidelity remain unverified. Detailed
      native evidence is in issue comment 6699086021. Launch disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5699
runtime:
  executable_path: Boka Battleships 1.0/Boka Battleships
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Boka%20Battleships%201.0.sit
    expected_sha256: 0ef973ee8bf877321e538da07e410a7e531923e5f1d6826cfc85ae13f24a0409
    expected_size: 259676
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Boka%20Battleships%201.0.sit
    - https://github.com/benletchford/systemless/issues/5699
    license: Original complete unchanged-distribution grant
    rights_holder: Marc Boxall
    permission: >-
      The original Read Me permits free and unrestricted distribution if all files
      remain unchanged and the Read Me and Register application accompany the game.
      Preserve the complete original archive, 30-day trial and registration restrictions.
    notes: >-
      Original version 1.0, copyright 1999. Read Me is 4446 bytes, SHA-256
      5513195ecb56ce363198b7c0d27cd4d85abb5e25d3e795f874adeaacd16cdba5.
      Independent extraction of the original StuffIt archive and its ST46 InstallerMaker
      payload matches all three installed files. The actual game has an empty data fork,
      a 155359-byte resource fork, six CODE resources (0 through 5) and no cfrg; 68K only.
      Game resource-fork SHA-256
      e60687889c0abe3e4bb5e1fbf9161014522220257be48eebd62192a043582cd1.
      The outer installer and Register application are not substituted for game architecture.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/boka-battleships/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5699
    permission: Original gameplay capture from the permitted unchanged shareware package.
    notes: >-
      Private native candidate 7bd49954 after moving a ship, confirming placement,
      firing a miss and receiving the computer's reply. Exact 420x270 crop of the main
      game content; excludes title bar, menu bar, desktop, emulator and host UI. No
      resampling or generated pixels. 18947 bytes, SHA-256
      ebad058070c8fa4cd3f8ae6eb9a11abe4d2b2166678a929c4691fd26f675e23d.
references:
- https://github.com/benletchford/systemless/issues/5699
---

![Boka Battleships after a shot and the computer's reply](incoming/boka-battleships/gameplay.png)

Choose an opponent, drag your ships into position, then click **Done**. Click the
opponent's grid to fire. The unchanged 1999 shareware game includes four
unregistered computer levels and its original 30-day trial.

Browser and released-runtime qualification are pending. The private native
candidate verifies opponent selection, ship movement, a shot and the computer's
reply. Victory, saving, two-human play and audio fidelity remain unverified.
