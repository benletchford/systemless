---
id: avara
kind: game
title: Avara
summary: Pilot a bipedal HECTOR through three-dimensional training and combat missions.
developer: Juri Munkki
publisher: Ambrosia Software
year: 1996
launch_enabled: true
architectures:
- 68k
default_architecture: 68k
category: FPS
compatibility:
  status: playable
  verified:
  - date: "2026-10-28"
    tester: Catalogue maintainer
    systemless_version: 1de46fee91b97548477f7a940248df803b75ecb6
    architecture: 68k
    environment: "Fresh native ci-test recheck on merged source 1de46fee91b97548477f7a940248df803b75ecb6, executable SHA-256 2e35b7da019d98b1f7a9d8eccd91a6fc36681dba59895fd8dd960702adfd89dd. Exact unchanged managed original and selector below, explicit classic 68K, depth 8, ordinary scheduler and fixed default time. Not Yet, name Pilot, Training Grounds, Start Server, Game > Start; held Up, Right and mouse firing. Six observed pixel checks and one non-silent audio check pass at 1340 frontend / 1916 guest ticks, zero exhaustion. A matched no-fire replay passes six checks including absence of the firing-colour sample; actual captures inspected. This is bounded movement, turning, firing-control feedback and audio-output evidence, not projectile hits, full missions, multiplayer, audio fidelity or independent-emulator equivalence."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4355#issuecomment-6443635939
  - date: "2026-10-28"
    tester: Catalogue maintainer
    systemless_version: 4959dbea1860ca8acb89823456a307594bcdc078
    architecture: 68k
    environment: "Browser launch approved after fresh release-profile local preview of public runtime source 4959dbea1860ca8acb89823456a307594bcdc078, with only a temporary local launch flag for testing. Chrome for Testing 156.0.8078.47 mac-arm64, ordinary worker/WebGL, 68K 25 MHz and max ticks per paint 2. Trusted inputs dismiss the intact trial, enter Pilot, select Training Grounds, start the local server and game, move, turn, and exercise held mouse firing/release; actual captures inspected. Two steady 20-second gameplay samples measure 54.634 / 54.417 guest ticks/sec against the unchanged minimum 50, 62.689 / 62.370 runtime replies/sec and audio queue minimums 486.71 / 487.21 ms. No runtime exceptions or console errors; no debug overlay, clock increase, throttle/security override or guest patch. Exact managed original bytes are delivered by the existing local CDP preview archive-redirection method because the asset host excludes localhost by CORS. Full missions, projectile hits, multiplayer, audible-playback fidelity and public released qualification remain unverified. Final committed-head replay and CI are recorded in the contribution before merge."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/4355#issuecomment-6444158927
  - date: "2026-10-27"
    tester: Catalogue maintainer
    systemless_version: 974aa935fec1322bd1341c4eae2dfa010093b2ec
    architecture: 68k
    environment: "Native ci-test callback-repair candidate #5171, executable SHA-256 40f7dfe467cfc41b981d94f3e5f1537cc3b38c9df735d2a9fd18e63d40234218. Exact original archive and executable selector below, ordinary scheduler, default display and fixed startup time. Choose Not Yet, enter a player name, select Training Grounds, start a server and choose Game > Start. Forward and right-arrow input change the viewpoint and reveal a course marker. Six measured pixel checks and one non-silent audio check during held mouse firing pass at 1590 frontend / 2166 guest ticks with zero exhausted frames. Captures inspected. The previous runtime fails the first geometry check and reports no HECTOR available. This is bounded native movement and audio-output evidence, not a completed mission, projectile-hit, multiplayer, audio-fidelity or browser qualification. PowerPC preference also selects 68K; no standalone PPC route is advertised."
    status: playable
    evidence: https://github.com/benletchford/systemless/pull/5171
runtime:
  executable_path: Avara 1.0.1/Avara 1.0.1 ƒ/Avara
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 136f1dff7198bbf4e0896223d4af462269610bbefea8547b48d657b6fd33ccfe
    size_bytes: 2476740
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
      prior written notice when the software is unmodified and the complete works are
      included. Distribution for profit requires written permission.
    notes: >-
      Complete unchanged Avara 1.0.1 StuffIt installer, retaining the game, missions,
      documentation, licence, helpers, models and editing tools. The original 30-day
      trial and registration requirements remain intact; no registration bypass or
      separately repackaged game files. Bundled licence data is 2469 bytes, SHA-256
      198db82577e60e4df0eef7f6f9376015f8a9270dc65aff2f9b3e3149b0bd34e0. Independent execution
      of the original installer verifies all 132 expanded payload paths and all 264
      data/resource forks against production extraction, with matching file types and
      creators. Finder flags are recorded separately; complete Finder-metadata parity is not
      claimed. The current native candidate displays the successful-installation
      message and Continue/Quit buttons. The author's later MIT source release is not used
      as permission for this original binary package. The application has 68K CODE
      resources and PPC acceleration modules; both CPU preferences select the 68K
      application route.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 9ad55219b2ec46e935e33e45ca8b0df8df9c97bf31c33036342b93bc02e7b4a1
    size_bytes: 3662
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
      Training Grounds. Extracted without scaling or generated artwork from the inspected
      800-by-600 capture, rectangle (150,264)-(650,584). Excludes desktop, window
      framing, other game windows and the Classic Mac menu bar.
references:
- https://github.com/jmunkki/Avara
- https://www.vintageapplemac.com/files/games/Avara%201.0.1.sit
---

![Avara gameplay](https://assets.systemless.org/catalogue/media/sha256/9a/9ad55219b2ec46e935e33e45ca8b0df8df9c97bf31c33036342b93bc02e7b4a1.png)

Pilot a **HECTOR**, a bipedal combat vehicle, through Avara's three-dimensional
missions. Use the arrow keys to move and turn, the mouse to aim, and its button
to fire. The **Window > Keyboard** editor lets you inspect and configure controls.

This is the original **Avara 1.0.1** shareware installer. Choose **Not Yet** at the
trial notice, enter a player name, select **Training Grounds**, click **Start
Server**, then choose **Game > Start**. The complete package retains its licence,
documentation, missions and editing tools. Its original 30-day trial terms apply.

Testing covers Training Grounds loading, movement, turning and firing controls
on the 68K route, natively and in a staged browser. Full missions, projectile
hits and multiplayer remain unverified.
