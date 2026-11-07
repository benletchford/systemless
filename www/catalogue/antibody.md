---
id: "antibody"
kind: "game"
title: "Antibody 1.0.0"
summary: "Pilot a microscopic ship through an alien body and destroy viruses."
developer: "Marco Carra"
publisher: "Slimyfrog Software"
year: 1997
architectures:
- 68k
- ppc
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
    environment: "Complete unchanged original archive, exact selector, explicit 68k and 800x600/depth 8. Both separately forced slices pass ordinary Later on the original reminder, Play, first scenario and live Level 1. Original Configure Keyboard confirms Keyboard-only mode, Up thrust, Left/Right movement and Space Bar shooting. Held Up/Left/Space moves and fires; equal-clock controls retain Up/Left and omit only Space. 68K active shows a hit/explosion and Score 10 against control 0; PPC active emits green projectiles absent in its control, with no PPC scoring claim. Fresh active/control repeats on each CPU pass nine measured pixels plus six exact own-run captures, fifteen checks each, at 1326 frontend ticks; actual 68K guest 1926, PPC guest 1326. All have zero exhausted frames. Actual native executable SHA-256 a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Rust/Cargo runtime source matches merged 3cf77d7d. Own-run consistency is not hardware-oracle accuracy or cross-CPU pixel parity. Hosted-browser input/pacing and released public play remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2308
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: ppc
    environment: "Complete unchanged original archive, exact selector, explicit ppc and 800x600/depth 8. Both separately forced slices pass ordinary Later on the original reminder, Play, first scenario and live Level 1. Original Configure Keyboard confirms Keyboard-only mode, Up thrust, Left/Right movement and Space Bar shooting. Held Up/Left/Space moves and fires; equal-clock controls retain Up/Left and omit only Space. 68K active shows a hit/explosion and Score 10 against control 0; PPC active emits green projectiles absent in its control, with no PPC scoring claim. Fresh active/control repeats on each CPU pass nine measured pixels plus six exact own-run captures, fifteen checks each, at 1326 frontend ticks; actual 68K guest 1926, PPC guest 1326. All have zero exhausted frames. Actual native executable SHA-256 a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Rust/Cargo runtime source matches merged 3cf77d7d. Own-run consistency is not hardware-oracle accuracy or cross-CPU pixel parity. Hosted-browser input/pacing and released public play remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2308
runtime:
  executable_path: "Antibody 1.0.0/Antibody"
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/antibody/original.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Antibody%201.0.0.sit
    - https://github.com/benletchford/systemless/issues/2308
    rights_holder: "Slimyfrog Software and credited contributors"
    permission: "Retained Software License section 2 permits non-profit distribution only when unaltered and complete; application About repeats free distribution without alteration or sale. Preserve every original file, licence, How to Register, installer log, Marco Carra and Slimyfrog credits, and original thirty-day trial. Original unregistered limits remain: two lives rather than five and automatic stop at level 15. No registration code, date reset or paid-feature patch is used."
    license: Original complete-package distribution grant
    notes: "All retained data/resource notices reviewed; no exclusive-media clause found. Eleven paths, twenty-two forks and eleven Finder records match independent extraction. Actual fat executable has CODE 0-2/cfrg and a 110462-byte PowerPC PEF data fork. Both CPU slices represent one distinct game. Original archive 2145256 bytes, SHA-256 b08c2bdc602cd5e6888776376e52ef9e41a2875985a7de8a20222a573a4557b4. Application resource fork 246403 bytes, SHA-256 ddb7b7f4b755fbbdfd3a242b3e538e0949e691f011ea6ce4bfdd68dede175e1c."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/antibody/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2308
    rights_holder: "Slimyfrog Software and credited contributors"
    permission: "Original gameplay capture from the permitted unchanged distribution."
    notes: "Exact 640x480 crop at (80,60) from the 68K active Level 1 field/HUD, excluding the surrounding game frame, menu bar, letterboxing and host UI."
references:
- https://github.com/benletchford/systemless/issues/2308
---

![Antibody 1.0.0 gameplay](incoming/antibody/gameplay.png)

Choose **Later** to try the original unregistered game, then **Play** and a
scenario. The original Keyboard-only setup uses **Up** to thrust, **Left/Right**
to move and **Space** to shoot. The original trial provides two lives and stops
at level 15. Later levels, limit traversal, registration features, persistence and
sound fidelity remain unverified.

Native movement/firing and matched controls are verified. Hosted-browser input
and pacing, and released public play remain unverified.
