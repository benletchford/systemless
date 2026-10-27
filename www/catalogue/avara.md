---
id: avara
kind: game
title: Avara
summary: Pilot a bipedal HECTOR through three-dimensional training and combat missions.
developer: Juri Munkki
publisher: Ambrosia Software
year: 1996
architectures:
- 68k
default_architecture: 68k
launch_enabled: false
category: FPS
compatibility:
  status: playable
  verified:
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: 974aa935fec1322bd1341c4eae2dfa010093b2ec
    architecture: 68k
    environment: >-
      Native ci-test callback-repair candidate #5171, executable SHA-256
      40f7dfe467cfc41b981d94f3e5f1537cc3b38c9df735d2a9fd18e63d40234218.
      Exact original archive and executable selector below, ordinary scheduler,
      default display and fixed startup time. Choose Not Yet, enter a player name,
      select Training Grounds, start a server and choose Game > Start. Forward and
      right-arrow input change the viewpoint and reveal a course marker. Six measured
      pixel checks and one non-silent audio check during held mouse firing pass at
      1590 frontend / 2166 guest ticks with zero exhausted frames. Captures inspected.
      The previous runtime fails the first geometry check and reports no HECTOR
      available. This is bounded native movement and audio-output evidence, not a
      completed mission, projectile-hit, multiplayer, audio-fidelity or browser
      qualification. PowerPC preference also selects 68K; no standalone PPC route
      is advertised.
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5171
runtime:
  executable_path: Avara 1.0.1/Avara 1.0.1 ƒ/Avara
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://www.vintageapplemac.com/files/games/Avara%201.0.1.sit
    expected_sha256: 136f1dff7198bbf4e0896223d4af462269610bbefea8547b48d657b6fd33ccfe
    expected_size: 2476740
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Avara%201.0.1.sit
    - https://github.com/benletchford/systemless/issues/4355
    license: Ambrosia Software original-package nonprofit distribution licence
    rights_holder: Juri Munkki and Ambrosia Software, Inc.
    permission: >-
      The bundled Avara License expressly permits nonprofit distribution without
      prior written notice when the software is unmodified and the complete works
      are included. Distribution for profit requires written permission.
    notes: >-
      Complete unchanged Avara 1.0.1 StuffIt installer, retaining the game, missions,
      documentation, licence, helpers, models and editing tools. The original
      30-day trial and registration requirements remain intact; no registration
      bypass or separately repackaged game files. Bundled licence data is 2469 bytes,
      SHA-256 198db82577e60e4df0eef7f6f9376015f8a9270dc65aff2f9b3e3149b0bd34e0.
      Independent execution of the original installer verifies all 132 expanded
      payload paths and all 264 data/resource forks against production extraction,
      with matching file types and creators. Finder flags are recorded separately;
      complete Finder-metadata parity is not claimed. The current native candidate
      displays the successful-installation message and Continue/Quit buttons.
      The author's later MIT source release is not used as permission for this
      original binary package. The application has 68K CODE resources and PPC
      acceleration modules; both CPU preferences select the 68K application route.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/avara/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4355
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Exact verified native replay of the original archive after forward input in
      Training Grounds. Extracted without scaling or generated artwork from the
      inspected 800-by-600 capture, rectangle (150,264)-(650,584). Excludes desktop,
      window framing, other game windows and the Classic Mac menu bar.
references:
- https://github.com/jmunkki/Avara
- https://www.vintageapplemac.com/files/games/Avara%201.0.1.sit
---

![Avara gameplay](incoming/avara/gameplay.png)

Pilot a **HECTOR**, a bipedal combat vehicle, through Avara's three-dimensional
missions. Use the arrow keys to move and turn, the mouse to aim, and its button
to fire. The **Window > Keyboard** editor lets you inspect and configure controls.

This is the original **Avara 1.0.1** shareware installer. Choose **Not Yet** at the
trial notice, enter a player name, select **Training Grounds**, click **Start
Server**, then choose **Game > Start**. The complete package retains its licence,
documentation, missions and editing tools. Its original 30-day trial terms apply.

Native testing covers mission loading, movement, turning and non-silent audio
output on the 68K route. Full missions, multiplayer and browser qualification
remain pending, so browser launch is disabled.
