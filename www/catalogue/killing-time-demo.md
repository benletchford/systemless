---
id: killing-time-demo
kind: game
title: Killing Time Demo
summary: >-
  Explore the haunted estate on Matinicus Island in the original Power Macintosh
  demo of this first-person shooter.
developer: Studio 3DO / Logicware
publisher: Logicware
year: 1997
architectures:
- ppc
default_architecture: ppc
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: b90ebadc9900e94d7fb1f3317dee880c8142d2a5 native candidate
    architecture: ppc
    environment: "Complete unchanged original demo, exact executable, PowerPC and depth 8. Initial display 800x600/depth 8; the original DrawSprocket context selects 640x480. Moving the pointer inside avoids the original top-edge menu pause. The title finishes into the live starting level. Holding Up for 60 ticks moves off the starting platform; Right for 30 ticks turns toward the harbour wall; Control for 30 ticks fires the revolver, leaving four rounds after settling. Equal-clock idle control omits those held keys and retains six rounds. Active and idle each pass ten measured pixels and five exact own-run image checks at frontend/guest tick 1350, with zero exhausted frames. Earlier captures match exactly before input; after movement 108863 pixels differ. Own-run consistency is not hardware-oracle accuracy. Native binary SHA-256 ab15a58f57bce0115d6e740bcaefe574a5dc99eb1fa1ad251b0f22f3122c3fce. The candidate includes File Manager and DrawSprocket fixes in #5734/#5736. Enemies, switches, ghost movies, level completion, persistence, audio fidelity and released public play remain unverified. Hosted-original browser evidence follows below."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5730
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 1b6f6164 private browser preview on b90ebadc runtime
    architecture: ppc
    environment: >-
      Fresh Chromium 156.0.8078.79/Linux aarch64 profiles, isolated worker/WebGL,
      SharedArrayBuffer, default 25 MHz and depth 8. Production COEP credentialless
      and COOP same-origin headers; actual private source stamp and architecture
      checked. The original hosted archive response is HTTP 200, 19349162 bytes,
      SHA-256 5af4f84ab4dd38663f610aa66783850e06a918cfb61195773e638b3d94aa6033.
      Both main and worker Wasm HTTP 200 responses match the build: 17835189 bytes,
      SHA-256 3a5a61e6ebeae2c67dd3cffb79ad83a487ec53d167e71a779288d1c8b4d16726.
      Trusted ordinary arrow and Control input moves, turns and fires; ammunition
      falls from six to five, while the no-held-key control retains six. Two
      unchanged 20-second samples cover live starting-level play, with periodic
      turns in the active case. Final steady guest/host snapshots are
      60.0848/60.0848 and 59.9374/59.9374 per second, maximum sampled worker frames
      24.3/23.4 ms and minimum audio queues 163.1/156.2 ms; no captured lifecycle or
      network errors. These snapshots are not whole-level averages. This approves
      bounded browser input and pacing for the unchanged demo. Other areas, ghost movies, complete
      levels, persistence, audio fidelity and unmodified public deployment remain
      unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5730
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: v0.111.0 public release
    architecture: ppc
    environment: >-
      Actual public HTTPS release in fresh Chromium 156/Linux aarch64 profiles, isolated
      worker/WebGL and SharedArrayBuffer, default 25 MHz/depth 8. No local hosting,
      resolver override or request interception. Public source stamp
      be8fb068119830d584ae5a099338c82d339bc856 verified. Original archive HTTP 200 matches
      the catalogue integrity pin. Main and worker Wasm HTTP 200 bodies match the
      independently downloaded deployed build: 15727694 bytes, SHA-256
      5ca3c00c8b65d216a4b9f87401cf28ac58bd95c58a85bd641ea481b6ea0e7c6e. Arrow keys move
      and turn; Control fires, leaving four rounds versus six in the omitted-key control.
      Enemy combat, switches, ghost movies, complete levels, persistence and audio
      fidelity remain unverified. Active and control each include an unchanged 20-second
      gameplay sample. Final steady guest/host snapshots are 59.9755/59.9755 and
      60.1504/60.1504 per second; maximum sampled worker frames 34.1/26.9 ms and minimum
      audio queues 153.7/171.2 ms. All unchanged pacing gates pass, with no captured
      lifecycle or network errors. These snapshots are not whole-level averages.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5730
runtime:
  executable_path: Killing Time™ Demo/Killing Time™ Demo
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 5af4f84ab4dd38663f610aa66783850e06a918cfb61195773e638b3d94aa6033
    size_bytes: 19349162
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/killing-time
    - https://classicmacdemos.com/download/killing-time/
    - https://github.com/benletchford/systemless/issues/5730
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Logicware; The 3DO Company
    permission: >-
      Maintainer-requested addition on 7 November 2026, using the established
      Systemless policy for unchanged, freely circulated promotional demos. This demo appeared
      on period Macintosh magazine cover discs. The bundled Readme contains no
      explicit redistribution grant or restriction; this policy is not an explicit publisher
      grant. Preserve the complete unchanged archive and remove it if a rights holder
      objects.
    notes: >-
      Preserve the Readme, Logicware and 3DO notices, game data, music and ghost
      movies. The original About screen identifies version 1.0.3, copyright 1997 Logicware,
      and Killing Time/Studio 3DO trademarks of The 3DO Company; it adds no
      distribution terms. The executable is PowerPC PEF with cfrg and no classic CODE slice.
      Independent extraction matches all 15 original paths, 30 forks and 15 supported
      Finder records against production packing; strict KPK end 31834922.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ac4e404a7bfe7ff53403f1bd8f38f4341d6e349759e73ff28faa508511624e5a
    size_bytes: 203560
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5730
    permission: >-
      Fresh content-only gameplay capture from the unchanged original demo. The
      underlying artwork remains its owners' property.
    notes: PowerPC starting level after movement, turning and firing; no host interface.
references:
- https://www.mobygames.com/game/6380/killing-time/releases/
- https://classicmacdemos.com/killing-time
- https://github.com/benletchford/systemless/issues/5730
---

## The haunted island

![The revolver and harbour in Killing Time Demo](https://assets.systemless.org/catalogue/media/sha256/ac/ac4e404a7bfe7ff53403f1bd8f38f4341d6e349759e73ff28faa508511624e5a.png)

Move the pointer into the game area and let the title finish to enter the demo.
Use the arrow keys to move and turn, Control to fire, Shift to run, and Command
with Left or Right to strafe. Press Space to activate switches, 1–7 to choose an
acquired weapon, and Escape for the main menu. The pointer at the top edge opens
menu access.
