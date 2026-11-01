---
id: heretic-demo
kind: game
title: Heretic
summary: >-
  Fight through D'Sparil's castle with a magic wand and an inventory of artefacts
  in Raven Software's dark-fantasy follow-up to Doom.
developer: Raven Software
publisher: id Software
year: 1996
architectures:
- ppc
default_architecture: ppc
category: FPS
compatibility:
  status: playable
  verified:
  - date: "2026-11-01"
    tester: Catalogue maintainer
    systemless_version: 80334667 deterministic play runner
    architecture: ppc
    environment: >-
      The unchanged archive ran natively on PowerPC. Escape, Return three times (New
      Game, episode, skill) and Escape reached the first level of the shareware
      episode. Holding Up and Left walked forward and picked up a wand crystal. A matched idle
      control was identical until the input and then differed by 208,136 pixels. An
      exact replay matched both checkpoints, with zero exhausted frames. Combat, later
      levels, and audio remain unverified. Browser pacing in gameplay reached 46–62 host
      fps with occasional 53–58 ms frames at 20–60 MHz pacing, so launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5495
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 34fc81b06fc1d1570542d9be8ebf1e06da9ea8d419cb46caad59f5096b622c5a
    size_bytes: 2905323
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/heretic
    - https://download.classicmacdemos.com/Heretic%200.9.2.sit
    - https://github.com/benletchford/systemless/issues/5495
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Raven Software and id Software
    permission: >-
      Hosting was accepted by the Systemless maintainer on 1 November 2026 under the
      policy for unchanged, freely circulated promotional demos. The unchanged demo
      appeared on period Macintosh magazine cover discs. The bundled documents contain no
      explicit redistribution grant or restriction. Preserve the complete unchanged
      archive and remove it if a rights holder objects.
    notes: >-
      Heretic 0.9.2 PowerPC Macintosh port bundled with Raven Software's freely
      distributable shareware heretic1.wad, which contains the first episode.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 757c0eca8ef7021e470dd0204b869fa6a9f0643f3c68e12e70c55c8c912fc400
    size_bytes: 32426
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5495
    permission: >-
      Fresh gameplay capture from the unchanged archive. The underlying game artwork
      remains its owners' property.
    notes: Native PowerPC first-level gameplay, cropped to the game window.
references:
- https://classicmacdemos.com/heretic
- https://github.com/benletchford/systemless/issues/5495
---

## Into the castle

![Heretic](https://assets.systemless.org/catalogue/media/sha256/75/757c0eca8ef7021e470dd0204b869fa6a9f0643f3c68e12e70c55c8c912fc400.png)

The Serpent Riders have conquered the land, and one elven wanderer is left to fight back. Explore D'Sparil's castle, gather artefacts and fight off gargoyles, golems and undead warriors.

Use the arrow keys to move and turn. Press Escape for the menu.
