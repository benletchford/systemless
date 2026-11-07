---
id: asterax
kind: game
title: Asterax 1.0.1
summary: >-
  Thrust, turn and blast asteroids to collect crystals in Arvandor’s arcade
  shooter.
developer: Michael Hanson
publisher: Arvandor Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original archive, exact selector, explicit 68k and
      800x600/depth 8. Ordinary one-player Begin and Control select Mantis. Original Set Keys
      confirms KP8 thrust, KP6/KP4 turn and Control fire. Held thrust/turn/fire moves and
      rotates the ship and emits a red projectile. Equal-clock no-fire control retains
      thrust/turn and omits only flight-stage Control, preserving ship-selection
      Control. Fresh active/control repeats each pass nine measured pixels plus four exact
      own-run captures, thirteen checks at 474 frontend/1074 guest ticks, zero
      exhaustion. Actual native executable SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Rust/Cargo runtime source matches merged 3cf77d7d. Own-run
      consistency is not hardware-oracle accuracy or cross-CPU pixel parity.
      Hosted-browser input/pacing and released public play remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2305
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 10147fc754a7e6f7ad1e37849834b5b3b9b3819e private browser candidate
    architecture: 68k
    environment: "Optimized clean private frontend 10147fc754a7e6f7ad1e37849834b5b3b9b3819e on merged runtime 3cf77d7d. Chromium 156.0.8078.79/Linux aarch64, isolated worker/WebGL, 25 MHz, 800x600/depth 8, deployed COEP credentialless/COOP same-origin and SharedArrayBuffer verified. Page build stamp and actual architecture checked. Untouched hosted original HTTP 200 response matches its complete archive size and SHA-256. Actual main and worker Wasm bodies match 17782297 bytes, SHA-256 e837fde08b3e24c7099479b6bb6e8c129250f2fcc043f01e29544581c23ba580. Ordinary Begin One Player and Control select Mantis. Trusted Numpad8/Numpad6/Control thrusts, turns and emits a red projectile. Matched control preserves pilot input and ship-selection Control but omits flight-stage firing; the projectile is absent. Both samples exercise four further pilot bursts and final captures retain the asteroid field. Unchanged 20-second active/control samples pass: final steady guest 62.1549/60.0853, host 61.1190/61.1212 per second, max frames 19.1/19.5 ms, min audio queue 130.8/131.4 ms, zero captured lifecycle/network errors. Final snapshots are not sample-wide means. Later waves, upgrades, two-player play, persistence, audio fidelity and public release remain unverified."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2305
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: v0.111.0 public release
    architecture: 68k
    environment: >-
      Actual public v0.111.0 HTTPS release, source
      be8fb068119830d584ae5a099338c82d339bc856, fresh Chromium 156/Linux aarch64,
      worker/WebGL/SAB, default 25 MHz/depth 8. No local hosting, resolver override or
      request interception. Original archive HTTP 200 matches 401668 bytes and SHA-256
      78bb57247d88f731426aaaa60e501b5cb41bc8d484268c5ff78fab20b70340d8. Main and worker
      Wasm HTTP 200 bodies match the independently downloaded deployment: 15727694 bytes,
      SHA-256 5ca3c00c8b65d216a4b9f87401cf28ac58bd95c58a85bd641ea481b6ea0e7c6e. Ordinary
      Begin One Player and Control select Mantis. Numpad8/Numpad6 thrust and turn; flight-
      stage Control emits a red projectile absent from the no-fire control. Both retain
      identical pilot and ship-selection inputs, exercise four further pilot bursts during
      unchanged 20-second gameplay samples, and finish in the asteroid field. Final steady
      guest/host snapshots are 61.8802/60.8489 and 61.8914/60.8599 per second; maximum
      sampled worker frames 18.5/19.0 ms and minimum audio queues 131.3/132.5 ms. All
      pacing gates pass with no captured lifecycle or network errors. Snapshots are not
      whole-wave averages or exact cross-case pixel comparisons. Later waves, upgrades,
      two-player play, persistence and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2305
runtime:
  executable_path: Asterax/Asterax
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 78bb57247d88f731426aaaa60e501b5cb41bc8d484268c5ff78fab20b70340d8
    size_bytes: 401668
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Asterax.sit
    - https://github.com/benletchford/systemless/issues/2305
    license: Original complete-package distribution grant
    rights_holder: Arvandor Software and credited contributors
    permission: >-
      Original README permits free distribution with the README and all standard
      application files, and prohibits distribution for profit without Arvandor Software
      consent. Preserve the complete unchanged package, shareware request, Michael Hanson
      programming, Anne Fullerton artwork and Sprite Animation Toolkit credits,
      preferences, scores, data, sounds, documentation and version notes.
    notes: >-
      All retained data/resource notices reviewed; no exclusive-media clause found.
      Eight paths, sixteen forks and eight Finder records match independent extraction.
      Actual resource-only game has CODE 0-6, no cfrg or native PPC slice; PowerMac
      compatibility in the README does not establish PPC code. Original archive 401668
      bytes, SHA-256 78bb57247d88f731426aaaa60e501b5cb41bc8d484268c5ff78fab20b70340d8.
      Application resource fork 268187 bytes, SHA-256
      5bc2291fd9d39c42ae6db067a7b33f846f6da5f193f11da508537ca71735ae98.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b920af52f0d9939fe371fc65e1f142cdd9413456beac01abd6b4663e3e9585ee
    size_bytes: 12027
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2305
    rights_holder: Arvandor Software and credited contributors
    permission: Original gameplay capture from the permitted unchanged distribution.
    notes: >-
      Original full 800x600 active game field, excluding menus, window framing,
      desktop and host UI; no resampling.
references:
- https://github.com/benletchford/systemless/issues/2305
---

![Asterax 1.0.1 gameplay](https://assets.systemless.org/catalogue/media/sha256/b9/b920af52f0d9939fe371fc65e1f142cdd9413456beac01abd6b4663e3e9585ee.png)

Choose **Begin Game — One Player**, then press **Control** to select a ship.
Use **keypad 8** to thrust, **keypad 4/6** to turn and **Control** to fire.
**Escape** returns to the menu. Later waves, purchases/upgrades, two-player play,
persistence and sound fidelity remain unverified.

Native and hosted-browser movement/firing and matched controls are verified.
Browser pacing passes. Released public play remains unverified.
