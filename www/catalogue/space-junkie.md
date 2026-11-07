---
id: space-junkie
kind: game
title: Space Junkie 1.2
summary: Move your ship and fire at alien formations in the original space shooter.
developer: Tuan Huynh
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original archive, exact selector, explicit 68k and
      800x600/depth 8. Ordinary Not Yet dismisses the original unregistered reminder; New Game
      and the original introduction reach Wave 1. Preferences confirm Mouse control.
      Pointer movement toward (200,524) moves the ship left; a held mouse button fires a
      blue projectile. Equal-clock no-fire control preserves pointer movement and omits
      only flight-stage mouse down/up, moving without that projectile. Fresh
      active/control repeats each pass nine measured pixels plus six exact own-run captures,
      fifteen checks at 964 frontend/1564 guest ticks, zero exhaustion. Early arrow-key
      attempts did not move the ship and are excluded. Actual native executable SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Rust/Cargo
      runtime source matches merged 3cf77d7d. Own-run consistency is not hardware-oracle
      accuracy or cross-CPU pixel parity. Hosted-browser input/pacing and released public
      play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2314
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 10147fc754a7e6f7ad1e37849834b5b3b9b3819e private browser candidate
    architecture: 68k
    environment: "Optimized clean private frontend 10147fc754a7e6f7ad1e37849834b5b3b9b3819e on merged runtime 3cf77d7d. Chromium 156.0.8078.79/Linux aarch64, isolated worker/WebGL, 25 MHz, 800x600/depth 8, deployed COEP credentialless/COOP same-origin and SharedArrayBuffer verified. Page build stamp and actual architecture checked. Untouched hosted original HTTP 200 response matches its complete archive size and SHA-256. Actual main and worker Wasm bodies match 17782297 bytes, SHA-256 e837fde08b3e24c7099479b6bb6e8c129250f2fcc043f01e29544581c23ba580. Ordinary Not Yet, New Game and the original introduction reach Wave 1. Trusted pointer movement/button input moves and fires a blue projectile; matched control preserves pointer movement without firing. Final Score 200 versus control 0; control ends at LIVES 0 and is not a continuously surviving board. Both unchanged 20-second gates fail the guest minimum: final steady guest 41.5211/47.2968, host 59.7499/59.3726 per second, max frames 30.8/22.6 ms, min audio queue 463.0/465.9 ms, zero captured lifecycle/network errors. Snapshots are not sample-wide means. Keep launch disabled / overall unknown. Later waves, keyboard/registration features, persistence, audio fidelity and public play remain unverified."
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2314
runtime:
  executable_path: Space Junkie 1.2/Space Junkie 1.2
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: c4d1e6c4ac930e213855f2cccb3d11c49a903975da065ef5ffc3381429f2c982
    size_bytes: 97063
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Space%20Junkie%201.2.sit
    - https://github.com/benletchford/systemless/issues/2314
    license: Original complete-package distribution grant
    rights_holder: Tuan Huynh and credited contributors
    permission: >-
      Original About Distribution section permits online-service, BBS, Internet,
      person-to-person and other non-commercial distribution with all original unaltered
      files. Profit or commercial-channel distribution requires written consent. Preserve
      the complete original game, About, Register Me and icon, shareware notices and
      registration behaviour.
    notes: >-
      All retained data/resource notices reviewed; no exclusive-media clause found.
      Four paths, eight forks and four Finder records match independent extraction.
      Actual game is resource-only, CODE 0-5, no cfrg/native PPC slice. Year follows the
      original release notice copyright range 1993-1995. Original archive 97063 bytes,
      SHA-256 c4d1e6c4ac930e213855f2cccb3d11c49a903975da065ef5ffc3381429f2c982.
      Application resource fork 135175 bytes, SHA-256
      7b495a32791e568173dfebdcbfcbbe3a8a432121f26d19ed798c61bfdd776701.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8058800ef02dfd89981a65b4c9e5f3c3122196e6c59498da28a0a810255ba903
    size_bytes: 3504
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2314
    rights_holder: Tuan Huynh and credited contributors
    permission: Original gameplay capture from the permitted unchanged distribution.
    notes: >-
      Exact 640x480 live field/HUD crop at (80,60), without resampling; excludes
      letterbox padding, menus, window framing and host UI.
references:
- https://github.com/benletchford/systemless/issues/2314
---

![Space Junkie 1.2 gameplay](https://assets.systemless.org/catalogue/media/sha256/80/8058800ef02dfd89981a65b4c9e5f3c3122196e6c59498da28a0a810255ba903.png)

Choose **Not Yet** to try the original unregistered game, then **New Game**.
Wait for the hyperspace and wave introduction. In the original Mouse mode, move
the mouse to steer and hold the mouse button to fire. Later waves, keyboard mode, registration features, persistence and sound fidelity
remain unverified.

Native and hosted-browser movement/firing are verified; browser Score reaches
200. Pacing misses the minimum, so launch stays disabled. Released public play
remains unverified.
