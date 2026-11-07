---
id: "space-junkie"
kind: "game"
title: "Space Junkie 1.2"
summary: "Move your ship and fire at alien formations in the original space shooter."
developer: "Tuan Huynh"
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: "Complete unchanged original archive, exact selector, explicit 68k and 800x600/depth 8. Ordinary Not Yet dismisses the original unregistered reminder; New Game and the original introduction reach Wave 1. Preferences confirm Mouse control. Pointer movement toward (200,524) moves the ship left; a held mouse button fires a blue projectile. Equal-clock no-fire control preserves pointer movement and omits only flight-stage mouse down/up, moving without that projectile. Fresh active/control repeats each pass nine measured pixels plus six exact own-run captures, fifteen checks at 964 frontend/1564 guest ticks, zero exhaustion. Early arrow-key attempts did not move the ship and are excluded. Actual native executable SHA-256 a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Rust/Cargo runtime source matches merged 3cf77d7d. Own-run consistency is not hardware-oracle accuracy or cross-CPU pixel parity. Hosted-browser input/pacing and released public play remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2314
runtime:
  executable_path: "Space Junkie 1.2/Space Junkie 1.2"
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/space-junkie/original.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Space%20Junkie%201.2.sit
    - https://github.com/benletchford/systemless/issues/2314
    rights_holder: "Tuan Huynh and credited contributors"
    permission: "Original About Distribution section permits online-service, BBS, Internet, person-to-person and other non-commercial distribution with all original unaltered files. Profit or commercial-channel distribution requires written consent. Preserve the complete original game, About, Register Me and icon, shareware notices and registration behaviour."
    license: Original complete-package distribution grant
    notes: "All retained data/resource notices reviewed; no exclusive-media clause found. Four paths, eight forks and four Finder records match independent extraction. Actual game is resource-only, CODE 0-5, no cfrg/native PPC slice. Year follows the original release notice copyright range 1993-1995. Original archive 97063 bytes, SHA-256 c4d1e6c4ac930e213855f2cccb3d11c49a903975da065ef5ffc3381429f2c982. Application resource fork 135175 bytes, SHA-256 7b495a32791e568173dfebdcbfcbbe3a8a432121f26d19ed798c61bfdd776701."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/space-junkie/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2314
    rights_holder: "Tuan Huynh and credited contributors"
    permission: "Original gameplay capture from the permitted unchanged distribution."
    notes: "Exact 640x480 live field/HUD crop at (80,60), without resampling; excludes letterbox padding, menus, window framing and host UI."
references:
- https://github.com/benletchford/systemless/issues/2314
---

![Space Junkie 1.2 gameplay](incoming/space-junkie/gameplay.png)

Choose **Not Yet** to try the original unregistered game, then **New Game**.
Wait for the hyperspace and wave introduction. In the original Mouse mode, move
the mouse to steer and hold the mouse button to fire. Enemy hits/scoring, later
waves, keyboard mode, registration features, persistence and sound fidelity are unverified.

Native movement/firing and matched controls are verified. Hosted-browser input
and pacing, and released public play remain unverified.
