---
id: erics-ultimate-solitaire-sample
kind: game
title: Eric’s Ultimate Solitaire Sample
summary: Play a Klondike hand in Delta Tao’s original solitaire sample.
developer: Delta Tao Software
publisher: Delta Tao Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt sample at 800 by 600. It opened an active
      Klondike table, and two clicks on the stock changed the waste card.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3634
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + release browser
    architecture: 68k
    environment: >-
      Fetched the exact archive once in a release-mode browser. The sample opened
      directly to a Klondike table, and repeated mouse clicks on the stock changed the
      visible waste card. A five-second active-table sample measured 60.0 host FPS and
      60.0 guest ticks/sec, with a 4.5 ms maximum frame and 131 ms minimum audio queue.
      The direct 635-by-384 gameplay capture excludes the website, menu bar, and window
      frame.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3634
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 040370f3b139c04affa6052805684e44c7c443f2546093b52a260a2aa28b7f1f
    size_bytes: 287384
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/erics-ultimate-solitaire
    rights_holder: Delta Tao Software, Inc.
    permission: >-
      The unchanged archive contains only Delta Tao’s purpose-built Eric’s Solitaire
      Sample application, with no bundled redistribution restriction. The sample was
      distributed on historical cover discs listed by the source catalogue. Its own text
      promotes purchasing the full version; no commercial-game files are included.
    notes: >-
      Original 287,384-byte StuffIt archive, SHA-256
      040370f3b139c04affa6052805684e44c7c443f2546093b52a260a2aa28b7f1f.
      The promoted public archive and gameplay screenshot were fetched back and
      matched their pinned SHA-256 hashes and byte lengths.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ce6766a0a8f5b27e09512ede952db0aaf9680eeb07abba00f25ad218072da654
    size_bytes: 63020
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3634
    permission: >-
      Fresh gameplay capture made from the original sample for this catalogue entry.
      The underlying game artwork remains its owners’ property.
    notes: >-
      Direct 635-by-384 capture of the active Klondike table after drawing from the
      stock, without website framing, Mac menu bar, or window frame.
references:
- https://classicmacdemos.com/erics-ultimate-solitaire
---

## Klondike sample

![Eric’s Ultimate Solitaire Klondike table](https://assets.systemless.org/catalogue/media/sha256/ce/ce6766a0a8f5b27e09512ede952db0aaf9680eeb07abba00f25ad218072da654.png)

The sample opens a Klondike table. Click the stock pile to turn cards, then
build the tableau and foundations using standard solitaire rules.
