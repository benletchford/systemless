---
id: bloodsuckers
kind: game
title: Bloodsuckers
summary: >-
  Defend your bare arm from swarms of bloodthirsty insects in Pangea Software's
  frantic arcade swatting gallery.
developer: Brian Greenstone
publisher: Pangea Software
year: 1993
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-16
    tester: Catalogue maintainer
    systemless_version: 0.41.7
    architecture: 68k
    environment: Deterministic headless run from the untouched author-hosted archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/165
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 381d618d8f6a1750162e89d7f152d94b6c7ff684be962f18f88d8f4921a46bc1
    size_bytes: 449249
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Bloodsuckers%20.sit
    - https://www.vintageapplemac.com/software/games
    license: Pangea Software shareware
    rights_holder: Pangea Software, Inc.
    permission: "The bundled documentation explicitly permits unrestricted distribution: \"Copy and distribute this program all you want. Shareware distributors are authorized to sell this program for no more than $5.\""
    notes: >-
      Untouched StuffIt archive retrieved from vintageapplemac.com. The 449,249-byte
      file has SHA-256
      381d618d8f6a1750162e89d7f152d94b6c7ff684be962f18f88d8f4921a46bc1. Brian Greenstone / Pangea Software remains the sole rights holder and has no
      active modern commercial releases or storefront listings for this title.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7810691791842c9c355f7d4c0bc198d71f6773f22b7622201deb230299ab2791
    size_bytes: 292959
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/165
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged archive on 2026-09-17 with
      authentic 8-bit palette mapping, cropped exactly to the game's 640-by-480 content. It
      excludes the Classic Mac menu bar, browser, website, host desktop and emulator
      chrome.
references:
- https://www.vintageapplemac.com/software/games
- https://www.vintageapplemac.com/files/games/Bloodsuckers%20.sit
---

## Swat or be drained

![Bloodsuckers gameplay](https://assets.systemless.org/catalogue/media/sha256/78/7810691791842c9c355f7d4c0bc198d71f6773f22b7622201deb230299ab2791.png)

Bloodsuckers turns arcade defense into an urgent reflex test. A human arm lies
exposed across the lower edge of the screen while bees, flies, gnats, mantises
and leeches drop from the dark background to latch on and feed. Controlling a
disembodied swatting hand with the mouse, the player must crush incoming pests
before their bites drain the vital blood supply.

Survival hinges on managing two competing gauges along the right flank: a rising
Pain meter and a falling Blood meter. Once an insect attaches, pain ticks upward
inexorably while blood drops at a rate proportional to the swarm's feeding frenzy.
Quick swatting preserves blood for post-wave bonuses, and clearing each round
unlocks high-speed bonus rounds to collect scattered blood packs.

## Fat binary and charity shareware

Programmed by Brian Greenstone with artwork by Dave Triplett, Bloodsuckers was
Pangea Software's debut Macintosh shareware release. Version 2.0 updated the
original 1993 game into a fat binary carrying both 68K and PowerPC code. In the
bundled documentation, Pangea championed the classic shareware ethos: rather than
demanding registration fees for themselves, Greenstone asked players to donate
their five-dollar fee directly to the American Cancer Society.

The original distribution archive is preserved here untouched. Systemless
extracts the StuffIt archive and launches the native application directly
without altering its files.
