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
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 26233b9b0ed9d5c9a13d270d5b7b2c2995cabe36
    architecture: 68k
    environment: >-
      Release-mode local Chrome preview, normal WebAssembly worker at 25 MHz,
      unchanged original archive. Stationary Don't Change and Not Yet clicks,
      then New Game, started the maze; normal Right-key down/up moved Blinky
      along the corridor. The textured maze background renders normally.
      Preview requests supplied independently hash-verified original bytes.
      No runtime or guest executable changes. Scoring, level completion,
      sustained play, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4391
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: 26233b9b0ed9d5c9a13d270d5b7b2c2995cabe36
    architecture: ppc
    environment: >-
      Release-mode local Chrome preview, normal WebAssembly worker at 25 MHz,
      unchanged original archive. Stationary Don't Change and Not Yet clicks,
      then New Game, started the maze; normal Right-key down/up moved Blinky
      along the corridor. The initial Sound Manager warning dismisses with OK. The maze background remains black; graphical fidelity issue #4361 stays open.
      Preview requests supplied independently hash-verified original bytes.
      No runtime or guest executable changes. Scoring, level completion,
      sustained play, saves and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4391
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: "0.83.0"
    architecture: 68k
    environment: Native headless replay of the unchanged Bubble Trouble 1.0.0 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
  - date: "2026-10-10"
    tester: Catalogue maintainer
    systemless_version: "0.83.0"
    architecture: ppc
    environment: >-
      Native headless replay of the unchanged archive after dismissing startup
      warnings
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4353
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 18066941cad15d94bc99c482ed0ded2308dc83e327366a36bf5bd4fc1e037f4e
    size_bytes: 1670970
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/b/
    - >-
      https://www.vintageapplemac.com/files/games/Bubble%20Trouble%201.0.0%20%C6%92.sit
    license: Ambrosia Software nonprofit distribution licence
    rights_holder: Ambrosia Software, Inc.
    permission: >-
      The bundled Bubble Trouble License permits nonprofit distribution without prior
      written notice when the software is unchanged and the complete works are
      included. Distribution for profit requires written permission.
    notes: >-
      Complete original 22-file archive, including its media, music plugins, Display
      Library, documentation and registration application. Original 30-day trial and
      registration notice retained without alteration.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: a55a4cb36e4ecc524051ae38550e4cdc385aaceec9204f59ab95ebe81cce9536
    size_bytes: 671774
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4353
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Exact original archive on 68K. Blinky responds to Right and moves along the
      corridor. Full game framebuffer excludes desktop and emulator UI.
references:
- https://www.vintageapplemac.com/software/games/b/
---

![Bubble Trouble gameplay](https://assets.systemless.org/catalogue/media/sha256/a5/a55a4cb36e4ecc524051ae38550e4cdc385aaceec9204f59ab95ebe81cce9536.png)

Guide Blinky through an underwater maze and collect treasure. Choose **Don't
Change** at the display prompt, **Not Yet** at the shareware notice, then
**New Game**. Right moves Blinky; controls can be configured in **Prefs**.

This complete original 1.0.0 package retains its 30-day shareware trial.
Bounded native and release-browser testing covers starting the maze and Right-key movement on both
68K and PPC. PPC additionally displays a Sound Manager warning; choose OK.
The PPC maze background is black rather than the texture visible on 68K.
Scoring, level completion, sustained play and saves remain unverified.
The normal browser player defaults to 68K. Release-browser preview tests used
independently hash-verified original archive bytes; production publication is
tracked separately in the catalogue ledger.
