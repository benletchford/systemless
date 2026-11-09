---
id: descent
kind: game
title: Descent
summary: >-
  Fly, roll and fight through fully three-dimensional mines in the Macintosh
  shareware release.
developer: Parallax Software
publisher: MacPlay / Interplay Productions
year: 1995
architectures:
- ppc
default_architecture: ppc
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: "0.112.0 (120dffb1)"
    architecture: ppc
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5898
    environment: >-
      Actual released desktop browser route, original three-level shareware,
      default PPC 25 MHz, Keyboard Only and Rookie. Pixel Double, Ship Autoleveling,
      display, partition and sound retain their original settings. Independent
      extraction matches all eighteen files, thirty-six forks and Finder records.
      The actual application is PPC PEF with cfrg and no CODE resources.
      Fresh private native source 9903e995 active/control repeats each pass
      sixteen checks at 1933 frontend/guest ticks with zero exhaustion; own
      references establish consistency, not independent hardware parity.
      Each initial twenty-second public sample begins before New Game and includes
      normal briefing continuation, first-mine initialization, 240 guest ticks
      settling, A acceleration and Return primary fire or omitted inputs.
      Input actions finish in 14941/15437 ms. The active ceiling-light area grows
      2.689 times versus 1.074 in the control; three of four predetermined firing
      captures show dual laser projectiles, versus none in the control. Actual
      input/control and final captures inspected. A separate full twenty-second
      sample remains in running first-mine gameplay, with four intermediate captures.
      All four unchanged pacing windows pass. Final steady guest rates, in order
      active initialization/sustained and control initialization/sustained, are
      59.8550/60.0333/59.8854/60.5933 ticks/sec; host rates are
      59.8550/61.0684/59.8854/60.5933 FPS. Maximum worker frame 24.7 ms,
      minimum audio queue 432.3 ms. Gates remain guest >=50, host >=55,
      frame <=50 ms and audio >=40 ms, with 1000 ms warmup. These are scheduling
      measurements, not a claim about game-rendered FPS. Actual archive, page and
      current dedicated-worker HTTP module bodies match pinned sizes and SHA-256.
      The current worker's loaded Wasm bytes also match; the debugger is disabled
      before measurement. All observed targets resumed, with zero lifecycle or
      observer errors and no concurrent heavy job. An untimed setup diagnostic
      precedes the first passing timed pair; no pacing failure was retried.
      Original game files and restrictions remain unchanged; no guest instructions
      are inspected. Full mine/level
      completion, combat accuracy, other levels/weapons, saves, multiplayer,
      independent visual parity and audio fidelity remain unverified.
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: f45c338df4bc4ceda38e6541f14b8dc93b543fd07d90a2c5d5118d2001c12ad2
    size_bytes: 4735288
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/descent
    - https://static.classicmacdemos.com/demos/descent/README.txt
    license: Descent shareware noncommercial distribution license
    rights_holder: Parallax Software and Interplay Productions
    permission: >-
      The included shareware license permits no-cost copying for personal,
      noncommercial use and sharing with other people. It excludes commercial distribution and
      paid bundles.
    notes: >-
      Unchanged 4,735,288-byte Macintosh shareware archive, SHA-256
      f45c338df4bc4ceda38e6541f14b8dc93b543fd07d90a2c5d5118d2001c12ad2.
- id: screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f43eb4635d1a4fdee7e62b147c8c40ec9ccdc184cb17a1180b0a19a6e173c9a9
    size_bytes: 139119
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://classicmacdemos.com/descent
    permission: >-
      Captured from the noncommercial shareware release; the game artwork remains the
      property of its rights holders.
    notes: >-
      Systemless capture of the opening level from the original shareware archive,
      cropped to the game content.
references:
- https://classicmacdemos.com/descent
---

## Six degrees of freedom

Descent replaces a flat floor plan with mines that twist in every direction.
The ship can pitch, roll and slide through tunnels while robots attack from
what used to be the ceiling.

This is the original PowerPC shareware package, not a repack or modern port.
Released-browser checks verify forward flight and primary-laser feedback in the first mine, with an omitted-input control and separate sustained gameplay on v0.112.0. Full levels, combat accuracy, saves, multiplayer and audio fidelity remain unverified.

The tested original keyboard bindings are **A** to accelerate and **Return** to fire the primary lasers. The verified setup uses **Keyboard Only** and the default **Rookie** difficulty.

![Descent opening level](https://assets.systemless.org/catalogue/media/sha256/f4/f43eb4635d1a4fdee7e62b147c8c40ec9ccdc184cb17a1180b0a19a6e173c9a9.png)
