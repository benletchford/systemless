---
id: "asterax"
kind: "game"
title: "Asterax 1.0.1"
summary: "Thrust, turn and blast asteroids to collect crystals in Arvandor’s arcade shooter."
developer: "Michael Hanson"
publisher: "Arvandor Software"
year: 1994
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
    environment: "Complete unchanged original archive, exact selector, explicit 68k and 800x600/depth 8. Ordinary one-player Begin and Control select Mantis. Original Set Keys confirms KP8 thrust, KP6/KP4 turn and Control fire. Held thrust/turn/fire moves and rotates the ship and emits a red projectile. Equal-clock no-fire control retains thrust/turn and omits only flight-stage Control, preserving ship-selection Control. Fresh active/control repeats each pass nine measured pixels plus four exact own-run captures, thirteen checks at 474 frontend/1074 guest ticks, zero exhaustion. Actual native executable SHA-256 a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Rust/Cargo runtime source matches merged 3cf77d7d. Own-run consistency is not hardware-oracle accuracy or cross-CPU pixel parity. Hosted-browser input/pacing and released public play remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2305
runtime:
  executable_path: "Asterax/Asterax"
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/asterax/original.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Asterax.sit
    - https://github.com/benletchford/systemless/issues/2305
    rights_holder: "Arvandor Software and credited contributors"
    permission: "Original README permits free distribution with the README and all standard application files, and prohibits distribution for profit without Arvandor Software consent. Preserve the complete unchanged package, shareware request, Michael Hanson programming, Anne Fullerton artwork and Sprite Animation Toolkit credits, preferences, scores, data, sounds, documentation and version notes."
    license: Original complete-package distribution grant
    notes: "All retained data/resource notices reviewed; no exclusive-media clause found. Eight paths, sixteen forks and eight Finder records match independent extraction. Actual resource-only game has CODE 0-6, no cfrg or native PPC slice; PowerMac compatibility in the README does not establish PPC code. Original archive 401668 bytes, SHA-256 78bb57247d88f731426aaaa60e501b5cb41bc8d484268c5ff78fab20b70340d8. Application resource fork 268187 bytes, SHA-256 5bc2291fd9d39c42ae6db067a7b33f846f6da5f193f11da508537ca71735ae98."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/asterax/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2305
    rights_holder: "Arvandor Software and credited contributors"
    permission: "Original gameplay capture from the permitted unchanged distribution."
    notes: "Original full 800x600 active game field, excluding menus, window framing, desktop and host UI; no resampling."
references:
- https://github.com/benletchford/systemless/issues/2305
---

![Asterax 1.0.1 gameplay](incoming/asterax/gameplay.png)

Choose **Begin Game — One Player**, then press **Control** to select a ship.
Use **keypad 8** to thrust, **keypad 4/6** to turn and **Control** to fire.
**Escape** returns to the menu. Later waves, purchases/upgrades, two-player play,
persistence and sound fidelity remain unverified.

Native movement/firing and matched controls are verified. Hosted-browser input
and pacing, and released public play remain unverified.
