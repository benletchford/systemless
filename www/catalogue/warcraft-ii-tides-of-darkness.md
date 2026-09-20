---
id: warcraft-ii-tides-of-darkness
kind: game
title: "Warcraft II: Tides of Darkness"
summary: Gather, build and command an army in Blizzard's classic real-time strategy demo.
developer: Blizzard Entertainment
publisher: Blizzard Entertainment
year: 1996
architectures: [68k, ppc]
default_architecture: 68k
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-20
    tester: Catalogue maintainer
    systemless_version: 0.42.1
    architecture: 68k
    environment: Deterministic headless gameplay run from the original shareware archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2222
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://classicmacdemos.com/download/warcraft-ii-tides-of-darkness/
    download_page: https://classicmacdemos.com/warcraft-ii-tides-of-darkness
    expected_sha256: 7b480153a0c15dcb084a201cd53edb35a99bbc0aea250c2e28e807ed6290ef3b
    expected_size: 10136636
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/warcraft-ii-tides-of-darkness
    - https://static.classicmacdemos.com/demos/warcraft-ii-tides-of-darkness/README.txt
    license: Blizzard Entertainment shareware distribution license
    rights_holder: Blizzard Entertainment
    permission: >-
      The Vendor text included in the package grants a nonexclusive right to
      distribute the complete, unchanged shareware program electronically at
      no charge. Commercial, retail, CD and bundled distribution require
      separate permission.
    notes: >-
      Unchanged 10,136,636-byte Macintosh demo archive, SHA-256
      7b480153a0c15dcb084a201cd53edb35a99bbc0aea250c2e28e807ed6290ef3b.
references:
- https://classicmacdemos.com/warcraft-ii-tides-of-darkness
---

## Build, scout, attack

Warcraft II turns a few workers and an unexplored map into a race for resources,
technology and position. The Macintosh demo supports 68040 and PowerPC systems;
this entry keeps Blizzard's complete original archive unchanged. The 68k build
reaches the live Hillsbrad campaign map and supports unit selection in
Systemless; the PowerPC path remains to be verified.
