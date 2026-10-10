---
id: bubble-trouble
kind: game
title: Bubble Trouble
summary: Guide Blinky through underwater mazes, bubbles and treasure.
developer: Alex Metcalf and David Wareing
publisher: Ambrosia Software
year: 1996
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: 2026-10-10
    tester: Catalogue maintainer
    systemless_version: 0.83.0
    architecture: 68k
    environment: Native headless replay of the unchanged Bubble Trouble 1.0.0 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
  - date: 2026-10-10
    tester: Catalogue maintainer
    systemless_version: 0.83.0
    architecture: ppc
    environment: Native headless replay of the unchanged archive after dismissing startup warnings
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Bubble%20Trouble%201.0.0%20%C6%92.sit
    expected_sha256: 18066941cad15d94bc99c482ed0ded2308dc83e327366a36bf5bd4fc1e037f4e
    expected_size: 1670970
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/b/
    license: Ambrosia Software nonprofit distribution licence
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      The bundled Bubble Trouble License permits nonprofit distribution without prior
      written notice when the software is unchanged and the complete works are
      included. Distribution for profit requires written permission.
    notes: >-
      Complete original 22-file archive, including its media, music plugins,
      Display Library, documentation and registration application. Original
      30-day trial and registration notice retained without alteration.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/bubble-trouble/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4353
    permission: Original gameplay screenshot captured for this catalogue at the maintainer's request.
    notes: >-
      Exact original archive on 68K. Blinky responds to Right and moves along
      the corridor. Full game framebuffer excludes desktop and emulator UI.
references:
- https://www.vintageapplemac.com/software/games/b/
---

![Bubble Trouble gameplay](incoming/bubble-trouble/gameplay.png)

Guide Blinky through an underwater maze and collect treasure. Choose **Don't
Change** at the display prompt, **Not Yet** at the shareware notice, then
**New Game**. Right moves Blinky; controls can be configured in **Prefs**.

This complete original 1.0.0 package retains its 30-day shareware trial.
Bounded native testing covers starting the maze and Right-key movement on both
68K and PPC. PPC additionally displays a Sound Manager warning; choose OK.
The PPC maze background is black rather than the texture visible on 68K.
Scoring, level completion, sustained play and saves remain unverified.
Launch stays disabled pending managed hosting and browser approval.
