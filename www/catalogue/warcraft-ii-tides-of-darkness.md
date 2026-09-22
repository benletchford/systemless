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
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic Human campaign run from the unchanged original StuffIt
      archive, with the same archive also launched through the title and menu
      sequence under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2416
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 7b480153a0c15dcb084a201cd53edb35a99bbc0aea250c2e28e807ed6290ef3b
    size_bytes: 10136636
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
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/warcraft-ii-tides-of-darkness/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2416
    permission: >-
      Original gameplay screenshot captured for this catalogue at the
      maintainer's request. Underlying Warcraft artwork remains the property
      of Blizzard Entertainment.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the exact
      unchanged demo archive on 2026-09-23 after entering the first playable
      Human mission and dismissing the tips panel. The 800x600 framebuffer was
      cropped exactly to the 640x480 game surface, excluding only the Classic
      Mac menu bar and uniform host margins; no game pixels were altered. PNG
      SHA-256 5bef60efe91af6696781c997373a97f762a245bd3e7ff42d896e3b2f0ac0fc34,
      148,956 bytes.
references:
- https://classicmacdemos.com/warcraft-ii-tides-of-darkness
---

## Build, scout, attack

![Warcraft II gameplay](incoming/warcraft-ii-tides-of-darkness/gameplay.png)

Warcraft II turns a few workers and an unexplored map into a race for resources,
technology and position. The Macintosh demo supports 68040 and PowerPC systems;
this entry keeps Blizzard's complete original archive unchanged.

The shareware campaign opens at Hillsbrad with a town hall, farm, workers and
an unexplored winter map. Its objectives retain the complete game's essential
rhythm: gather gold and lumber, establish a barracks and additional farms, then
turn a vulnerable settlement into a functioning military outpost.

Systemless accepts the original startup options, renders the animated
introduction, navigates the campaign briefing and reaches live mission play
with units, resources, construction, fog-of-war and the minimap active. The
same unchanged archive launches through the title and menu sequence under
BasiliskII. The 68k application is verified; the PowerPC path remains to be
verified.

## Blizzard's original shareware release

This is Blizzard's deliberately distributed Macintosh shareware demo, not the
retail game. The bundled Vendor text grants electronic redistribution of the
complete, unchanged shareware program without charge, while reserving retail,
CD and bundled distribution. The catalogue preserves that exact archive and
does not extend the permission to modified copies or commercial media.
