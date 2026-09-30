---
id: mah-jong-parlour-demo
kind: game
title: Mah Jong Parlour Demo
summary: Play a four-player mahjong hand against computer opponents.
developer: Exitdata Software
publisher: Aspyr Media
year: 1996
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
      Replayed the unchanged StuffIt demo, started New Game, reached the Macintosh
      User's turn, selected a tile, and discarded it with Space.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3656
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + release-mode browser build
    architecture: 68k
    environment: >-
      In Chrome, fetched the unchanged archive once, started New Game, reached the
      Macintosh User's hand, and advanced the turn by selecting a tile and pressing
      Space. A five-second active-play sample measured 60.0 host frames/s, 60.2 guest
      ticks/s, 0.9 ms maximum frame, and 131 ms minimum audio queue. The opening deal ran
      more slowly, about 14 guest ticks/s during one sample, before reaching the playable
      hand.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3656
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 2b1fef34ec366b16b8999200fbbf48c48ba54d28e1c87679b802e2eb1ee8dcfe
    size_bytes: 1648246
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/mah-jong-parlour
    rights_holder: Exitdata Software / Aspyr Media
    permission: >-
      The bundled Demo ReadMe expressly permits distributing this demo with the
      ReadMe and its accompanying files, all of which are in this unchanged archive. It
      prohibits sale or commercial distribution without consent; this catalogue offers the
      demo without charge.
    notes: >-
      Original 1,648,246-byte StuffIt archive, SHA-256
      2b1fef34ec366b16b8999200fbbf48c48ba54d28e1c87679b802e2eb1ee8dcfe.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 18434b8377a9b1efb76ff612d2b3dea580f406ddc2ec1b6792b832a4989910f2
    size_bytes: 17449
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3656
    permission: >-
      Fresh gameplay capture made from the original demo for this catalogue entry.
      The underlying game artwork remains its owners' property.
references:
- https://classicmacdemos.com/mah-jong-parlour
---

## Build a winning hand

![Mah Jong Parlour table](https://assets.systemless.org/catalogue/media/sha256/18/18434b8377a9b1efb76ff612d2b3dea580f406ddc2ec1b6792b832a4989910f2.png)

Click the title screen, then choose File → New Game. After the tiles are dealt,
your hand is along the bottom of the table. Select a tile and press Space to
discard it; use the buttons at lower right to respond to opponents' discards.
The opening deal can take a minute or two.
