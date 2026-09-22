---
id: simtower
kind: game
title: "SimTower: The Vertical Empire"
summary: >-
  Build upward, balance tenants and learn the foundations of tower management in
  Maxis's interactive demo.
developer: OPeNBooK Co., Ltd.
publisher: Maxis
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic lobby-and-office construction run from the unchanged original
      StuffIt demo archive, cross-checked through the same script under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2418
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 3543df4f814e215a040c8dfb14ff462c3862c1b21ea6eec696ce520c32bb8e99
    size_bytes: 594043
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/simtower-the-vertical-empire
    - https://gamefaqs.gamespot.com/mac/564236-simtower/faqs/2168
    license: Maxis SimTower promotional demo distribution
    rights_holder: OPeNBooK Co., Ltd. and Maxis / Electronic Arts
    permission: >-
      Maxis deliberately distributed this self-contained interactive demo through
      online services, including Info-Mac, and authorised its inclusion on numerous
      contemporary cover discs. This entry preserves that exact unchanged demo; it does not
      include or claim permission for the retail game.
    notes: >-
      Unchanged 594,043-byte StuffIt archive, SHA-256
      3543df4f814e215a040c8dfb14ff462c3862c1b21ea6eec696ce520c32bb8e99. It contains one resource-only 68K application
      named “SimTower Interactive Demo”.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0a91176d3435e3cca39abb5863412c8688a9f118750c1450060099b951ae4a34
    size_bytes: 220284
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2418
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request. Underlying SimTower artwork remains the property of its rights holders.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the exact unchanged
      demo archive on 2026-09-23 after constructing and expanding a lobby and placing
      three offices. The complete 800x600 guest framebuffer is preserved without alteration
      or host chrome. PNG SHA-256
      0a91176d3435e3cca39abb5863412c8688a9f118750c1450060099b951ae4a34, 220,284 bytes.
references:
- https://classicmacdemos.com/simtower-the-vertical-empire
- https://gamefaqs.gamespot.com/mac/564236-simtower/faqs/2168
---

## Start at street level

![SimTower gameplay](https://assets.systemless.org/catalogue/media/sha256/0a/0a91176d3435e3cca39abb5863412c8688a9f118750c1450060099b951ae4a34.png)

An empty skyline and two million dollars leave every important decision to the
player. The first lobby defines the footprint. Offices need access, stairs and
eventually elevators; each new tenant turns a simple stack of floors into a
traffic problem that unfolds over an entire simulated day.

The interactive demonstration teaches that loop directly. It asks the player
to lay out a lobby, extend it across the site, place offices and connect the new
floor. Funds change with every construction choice, while the compact tool
palette and evaluation tabs keep the tower's physical and financial state in
view.

## Maxis's interactive demonstration

This is the original `SimTower Interactive Demo`, not the commercial game. A
contemporary 1995 SimTower FAQ directed Macintosh players to Maxis's demo on
Info-Mac, and the preserved package appeared on at least thirteen magazine and
software-library discs. The catalogue keeps that complete 594,043-byte archive
unchanged.

Systemless opens its resource-only 68K application, reaches the live building
tutorial and accepts construction input. A deterministic run creates and
widens the lobby, selects the office tool and places three offices; the same
script reaches the matching tower state under BasiliskII.
