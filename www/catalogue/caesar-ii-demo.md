---
id: caesar-ii-demo
kind: game
title: Caesar II Demo
summary: Build a Roman city in Sierra's time-limited Macintosh demonstration.
developer: Impressions Games
publisher: Sierra On-Line
year: 1996
architectures: [68k]
default_architecture: 68k
category: Simulation
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: 2026-09-28
    tester: Catalogue maintainer
    systemless_version: 2283b95e25a606bc056b4e04b67240fb56c3684a
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged StuffIt demo. The title and
      introduction opened, the initial city map loaded, and its calendar advanced
      from January to March after the final message was dismissed. Successful
      building placement, native-Mac comparison, and browser launch have not yet
      been verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3075
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Caesar%20II%20Demo.sit
    expected_sha256: fe964cf06291942749ad8f9382075ff671736493df90dc64f8a48468f900fa27
    expected_size: 4419345
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/caesar-ii
    - https://download.classicmacdemos.com/Caesar%20II%20Demo.sit
    - https://static.classicmacdemos.com/demos/caesar-ii/README.txt
    rights_holder: Caesar II rights holders
    permission: >-
      This is Sierra's unchanged purpose-built playable demonstration, not the
      retail game. The included ReadMe describes its ten-year or 1,000-person
      limit, disabled saves, and omitted full-game features. No express
      redistribution prohibition appears in that ReadMe. This preservation
      rationale applies only to the original promotional demo archive.
    notes: >-
      Original 4,419,345-byte StuffIt archive, SHA-256
      fe964cf06291942749ad8f9382075ff671736493df90dc64f8a48468f900fa27.
      Its ReadMe lists a 68040 as the minimum Mac CPU and PowerPC as a faster
      option; the archive includes the demo application and its supporting data.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/caesar-ii-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3075
    permission: >-
      Fresh gameplay capture made for this catalogue entry from the unchanged
      promotional demo. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 796-by-534 game-content crop at (2,66) of a deterministic
      800-by-600 Systemless framebuffer after the city map became interactive.
      The Mac menu bar, desktop, and window title were excluded without changing
      game pixels. PNG SHA-256
      b8c8e4d7645955965c9351b269f0212ed15c239caec06545fb6c06e5f5009136,
      429,011 bytes.
references:
- https://classicmacdemos.com/caesar-ii
- https://static.classicmacdemos.com/demos/caesar-ii/README.txt
---

## A frontier city

![Caesar II demo city map and building controls](incoming/caesar-ii-demo/gameplay.png)

The demonstration opens on an undeveloped Roman province with a river,
scattered trees, a geography map, and building controls. Once its introductory
messages are dismissed, the calendar advances and the player can inspect the
terrain and available construction tools.

This is the original Macintosh playable demo, not the retail game. Sierra's
included ReadMe says the demo ends after ten years or when the population
reaches 1,000, and saves are disabled. Systemless reaches the live city map
from the unchanged 68K-compatible archive. Browser launch remains disabled
until a manual check is complete.
