---
id: warcraft-orcs-humans
kind: game
title: "Warcraft: Orcs & Humans"
summary: >-
  Build an outpost and command the Horde in Blizzard's playable Macintosh
  demonstration.
developer: Blizzard Entertainment
publisher: Blizzard Entertainment
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.50.0"
    architecture: 68k
    environment: >-
      Deterministic Orc campaign run from the unchanged original StuffIt demo
      archive, with the same archive also launched under BasiliskII
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2412
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 3b3648e199390eed9f66817ffd62f1d809541f1d184032a37b4e5de0d3e2f5ab
    size_bytes: 1935845
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/warcraft-i-orcs-humans
    - >-
      https://news.blizzard.com/en-us/article/24055599/get-warcraft-orcs-humans-on-battle-net-now
    - https://www.blizzard.com/legal/
    license: Blizzard Entertainment Warcraft promotional demo distribution
    rights_holder: Blizzard Entertainment, Inc.
    permission: "Blizzard deliberately released this self-contained package as the Warcraft: Orcs & Humans demo. Its bundled “Demo Read Me” identifies version 1.0.7 and the Product Demos distribution channel, while the game presents ordering information for the complete product. Contemporary magazine cover-disc appearances provide further evidence of deliberate public demo distribution. That evidence supports preservation of this exact unchanged promotional archive; it does not extend to retail media, modified copies or Blizzard's current commercial release."
    notes: >-
      The unchanged 1,935,845-byte StuffIt archive has SHA-256
      3b3648e199390eed9f66817ffd62f1d809541f1d184032a37b4e5de0d3e2f5ab. It contains the Warcraft Demo
      application, War Demo Data, War Demo Movies and Blizzard's original Demo Read Me. The
      Read Me identifies version 1.0.7 and was last updated 11 June 1996. Blizzard
      continues to sell the complete game; the catalogue hosts only this historical
      promotional demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 9d33173efa044ea61901f578058ed9fa0c3cfbf63f5b2363c841070cac663ac6
    size_bytes: 143993
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2412
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request. Underlying Warcraft artwork remains the property of Blizzard
      Entertainment, Inc.
    notes: >-
      Fresh deterministic Systemless 0.50.0 capture made from the exact unchanged
      demo archive on 2026-09-23 after entering the first playable Orc mission. The
      800x600 framebuffer was cropped exactly to the 640x480 game surface, excluding only
      uniform host margins; no game pixels were altered. PNG SHA-256
      9d33173efa044ea61901f578058ed9fa0c3cfbf63f5b2363c841070cac663ac6, 143,993 bytes.
references:
- https://classicmacdemos.com/warcraft-i-orcs-humans
- >-
  https://news.blizzard.com/en-us/article/24055599/get-warcraft-orcs-humans-on-battle-net-now
- https://www.blizzard.com/legal/
---

## Raise an Orc outpost

![Warcraft: Orcs & Humans gameplay](https://assets.systemless.org/catalogue/media/sha256/9d/9d33173efa044ea61901f578058ed9fa0c3cfbf63f5b2363c841070cac663ac6.png)

Blackhand sends the player into the Swamps of Sorrow with a direct assignment:
construct six farms and a barracks while keeping the outpost defended. Peons
gather lumber and gold, buildings turn those resources into a functioning base,
and Orc units explore the darkened map around the settlement.

The demonstration preserves the deliberate pace of the original real-time
strategy game. Every new structure competes for limited resources, unexplored
terrain hides both opportunities and danger, and the player has to balance
expansion against defence rather than simply directing a single unit.

## Blizzard's Macintosh demonstration

This is Blizzard's original Macintosh promotional demo, not the complete game
and not the current Battle.net release. Its bundled file is explicitly titled
“Demo Read Me,” identifies the application as Warcraft version 1.0.7 and records
its Product Demos distribution channel. The program itself includes an ordering
screen for the complete version, and the same package circulated on multiple
contemporary magazine cover discs.

Systemless opens the unchanged StuffIt archive, accepts the original startup
options, plays the animated battle, navigates the main menu and campaign
briefing, and reaches live Orc mission play with units, resources, construction
and the minimap active. The same archive launches under BasiliskII. The launcher
remains disabled until the archive and screenshot complete asset promotion and
browser review.
