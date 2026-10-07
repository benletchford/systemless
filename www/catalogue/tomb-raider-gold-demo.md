---
id: tomb-raider-gold-demo
kind: game
title: Tomb Raider Gold Demo
summary: >-
  Explore part of Tomb Raider Gold's second level in the original PowerPC Mac
  demo.
developer: Westlake Interactive
publisher: Aspyr Media
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-10-07"
    tester: Catalogue maintainer
    systemless_version: 0.81.2 deterministic play runner and browser preview
    architecture: ppc
    environment: >-
      The original demo reaches the live 3D level and accepts forward movement. A
      local browser preview reaches the level and held Up input advances Lara. Longer play
      and period Macintosh fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4174
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: local_file
    sha256: b7a5fd3e3e2f529bc8e7a4fce0c65d54033638e7baf104d14debf24da664d4c0
    size_bytes: 2724762
  provenance:
    redistribution: unknown
    original: true
    sources:
    - https://www.application-systems.de/tombraider/tr1/
    notes: Exact original 2,724,762-byte demo archive supplied by the visitor; not hosted.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b9fdba2b94dfc1646d42c82bfe7aabd64996da9e3b7d4aebe950661a520782ed
    size_bytes: 271695
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4174
    permission: >-
      Fresh content-only gameplay capture made from the original demonstration
      archive for this catalogue entry. The underlying game artwork remains its owners'
      property.
references:
- https://www.application-systems.de/tombraider/tr1/
- https://github.com/benletchford/systemless/issues/4174
---

## Enter the lost ruins

![Lara in the Tomb Raider Gold demo](https://assets.systemless.org/catalogue/media/sha256/b9/b9fdba2b94dfc1646d42c82bfe7aabd64996da9e3b7d4aebe950661a520782ed.png)

Choose your own legally obtained original Tomb Raider Gold demo archive. The
[historic distributor announcement](https://www.application-systems.de/tombraider/tr1/)
documents the Power Macintosh demo, but no archive is hosted by Systemless.
The browser checks the archive's exact size and SHA-256 before starting it.

Choose **Game**, then **New Game** with Return. Use the arrow keys to move and
turn Lara. The bundled Read Me lists Command for jump and Space for drawing
weapons.
