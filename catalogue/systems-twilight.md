---
id: systems-twilight
kind: game
title: System's Twilight
summary: >-
  Cross a fallen computer world where every quarrelling program guards a puzzle
  and every solved machine restores a little order to the System.
developer: Andrew Plotkin
publisher: Sadistic Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-16
    tester: Catalogue maintainer
    systemless_version: 0.41.4
    architecture: 68k
    environment: Deterministic headless run from the author-hosted original archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/1992
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 87c3dc9ad8c2d7317a776a8673c208e2c6e0c58814983eb6870fae1a54dadc31
    size_bytes: 787231
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://eblong.com/zarf/twilight/
    - https://eblong.com/zarf/ftp/systems-twilight-110.hqx
    license: Freeware for non-commercial distribution
    rights_holder: Andrew Plotkin
    permission: >-
      The included READ ME FIRST permits anyone to copy or distribute the program
      freely when no money is charged. Sale or inclusion in a for-profit collection
      requires the author's written permission.
    notes: >-
      Untouched System's Twilight 1.1.0 BinHex archive retrieved from Andrew
      Plotkin's current website on 2026-09-16. The 787231-byte file has SHA-256
      87c3dc9ad8c2d7317a776a8673c208e2c6e0c58814983eb6870fae1a54dadc31. The author-hosted page was
      updated on 2026-05-06 and identifies this 2000 build as the last game release. Its
      newer Mini vMac bundles are not hosted because the author says they include an
      unlicensed Mac ROM.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: bf9cb68d2dd7434cd260ce14a32bfd47e457e4b32b88419e7b8a6b9a7dfbc130
    size_bytes: 29473
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/154
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged author-hosted archive on
      2026-09-16, cropped exactly to the application's content window. It excludes the Classic
      Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://eblong.com/zarf/twilight/
- https://eblong.com/zarf/twilight/design-notes.html
---

## A fairy tale inside the machine

![System's Twilight gameplay](https://assets.systemless.org/catalogue/media/sha256/bf/bf9cb68d2dd7434cd260ce14a32bfd47e457e4b32b88419e7b8a6b9a7dfbc130.png)

The System was once orderly. By the time you arrive, its Powers have quarrelled,
its pathways have broken into neon fragments, and a small red figure is left to
pick through the consequences. Conversations unfold like scraps of an old myth,
only the gods are programs and their ruined kingdom lies on the other side of the
screen.

Movement is simple: click where the figure can walk or jump. The world around it
is not. One room might be a machine to understand, another a pattern to unpick,
and another a rule that only becomes visible after several wrong ideas. Solved
places stay solved, so the map gradually turns confusion into a record of little
victories.

## The last Macintosh release

Andrew Plotkin made version 1.1.0 freeware in 2000, removing the old registration
screen and leaving the complete game open to play. This is the unchanged BinHex
archive that he still hosts on the game's official page.

The package remains intact. Systemless opens the BinHex and StuffIt layers and
launches the 68K application directly; no emulator bundle, disk image, Mac ROM or
repacked application has been substituted.
