---
id: war-of-flowers
kind: game
title: War of Flowers 1.1
summary: >-
  Collect matching flower cards, build a score and decide when to stop in Minho
  Choi's three-player oriental card game.
developer: Minho Choi
publisher: Minho Choi
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: Deterministic gameplay run from the complete unchanged War of Flowers 1.1 archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2316
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/War%20of%20Flowers.sit
    download_page: https://www.vintageapplemac.com/software/games/w/
    expected_sha256: 785223705c65ebd6c9972a8d6d66fe5fa8397a2f7d4ea454a65fab074bd34954
    expected_size: 105469
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/w/
    - https://www.vintageapplemac.com/files/games/War%20of%20Flowers.sit
    license: War of Flowers 1.1 Read Me distribution terms
    rights_holder: Minho Choi
    permission: >-
      The bundled Read Me states: “Distribution for profit is prohibited without
      written permission by the author. This software may be placed on online
      services as long as only normal connect fees are charged. Do not distribute
      without accompanying documents. Do not distribute altered copy.” This entry
      therefore retains the complete, unchanged archive for the permitted
      online-service, normal-connect-fee distribution condition.
    notes: >-
      Unchanged 105,469-byte StuffIt archive, pinned by SHA-256
      785223705c65ebd6c9972a8d6d66fe5fa8397a2f7d4ea454a65fab074bd34954. The
      complete package retains the 68k application, Read Me, Release Note,
      comments and icon.
references:
- https://www.vintageapplemac.com/software/games/w/
- https://www.vintageapplemac.com/files/games/War%20of%20Flowers.sit
---

## A war of flowers

War of Flowers is a three-player oriental card game. Match cards by month,
collect scoring sets, and decide whether to call GO for more points or STOP to
secure the current game before another player can win.

## Preserved with its distribution terms

The bundled Read Me prohibits distribution for profit without the author's
permission, permits placement on online services when only normal connect fees
are charged, requires the accompanying documents, and forbids altered copies.
This entry references the unchanged complete archive with the application,
documentation and icon intact.

## In the live board game

Systemless reaches the first-player selection, the live board and the player's
hand from the exact archive. After selecting the first player, a deterministic
run selected a card from the player's hand; the before/after frames show the
board and hand changing as the turn proceeds, verifying gameplay beyond launch
and menu navigation.
