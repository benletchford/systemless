---
id: marathon-2-durandal
kind: game
title: "Marathon 2: Durandal"
summary: Fight across Lh'owon as Durandal pursues the secrets of the S'pht civilization.
developer: Bungie
publisher: Bungie
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: FPS
launch_enabled: true
aliases:
- /marathon2
- /marathon-2
compatibility:
  status: playable
  verified:
  - date: "2026-11-09"
    tester: Catalogue maintainer
    systemless_version: "0.112.0 (120dffb1)"
    architecture: 68k
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5302#issuecomment-6740264170
    environment: >-
      Actual released route, original Trilogy Release archive, default 25 MHz,
      original display, partition, preferences and sound; no overrides or game
      changes. Independent extraction matches nine files, eighteen forks and
      Finder records with strict end-of-pack validation. Fresh committed-native
      source 9903e995 input/control repeats each pass eight checks with zero
      exhaustion; own references establish consistency, not hardware parity.
      Each public twenty-second initialization window starts before New Game,
      includes stable chapter continuation and first-level initialization, settles
      240 guest ticks, then sends keypad 8 for sixty guest ticks or omits it.
      A populated room is required before comparing each run's own wall geometry;
      initial headings differ across browser cases. The active view moves while
      the omitted-input wall mask retains IoU 1.0. A separate full twenty-second
      window remains in running first-level gameplay with four intermediate
      captures. Actual input, control and final captures inspected. All four
      unchanged pacing windows pass; final steady guest rates in order active
      initialization/sustained, control initialization/sustained are
      60.9521/60.0310/53.8796/53.9243 ticks/sec; host rates
      59.9362/60.0310/59.9792/60.0289 FPS. Maximum worker frame
      24.3 ms; minimum audio queue 125.7 ms. Gates remain guest >=50,
      host >=55, frame <=50 ms and audio >=40 ms, with 1000 ms warmup. Actual
      archive and page module HTTP bodies match pinned sizes and SHA-256.
      The current dedicated worker HTTP module body matches the same released
      15854884-byte Wasm, SHA-256
      eee14504e744db7a2dc780f968cc2a5a9ee6f7c0fabf601d498a48b2df750c55.
      All targets resumed, with zero lifecycle/observer errors and no concurrent
      heavy job. Earlier readiness and observer setup failures are retained in
      the issue; no unchanged measured pacing failure was retried. Complete
      levels/campaign, combat accuracy, saves, multiplayer, independent visual
      parity and audio fidelity remain unverified.
  - date: "2026-11-09"
    tester: Catalogue maintainer
    systemless_version: "0.112.0 (120dffb1)"
    architecture: ppc
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5302#issuecomment-6740264170
    environment: >-
      Actual released route, original Trilogy Release archive, default 25 MHz,
      original display, partition, preferences and sound; no overrides or game
      changes. Independent extraction matches nine files, eighteen forks and
      Finder records with strict end-of-pack validation. Fresh committed-native
      source 9903e995 input/control repeats each pass eight checks with zero
      exhaustion; own references establish consistency, not hardware parity.
      Each public twenty-second initialization window starts before New Game,
      includes stable chapter continuation and first-level initialization, settles
      240 guest ticks, then sends keypad 8 for sixty guest ticks or omits it.
      A populated room is required before comparing each run's own wall geometry;
      initial headings differ across browser cases. The active view moves while
      the omitted-input wall mask retains IoU 1.0. A separate full twenty-second
      window remains in running first-level gameplay with four intermediate
      captures. Actual input, control and final captures inspected. All four
      unchanged pacing windows pass; final steady guest rates in order active
      initialization/sustained, control initialization/sustained are
      59.9322/60.0705/59.9312/60.0045 ticks/sec; host rates
      59.9322/60.0705/62.0341/60.0045 FPS. Maximum worker frame
      40.9 ms; minimum audio queue 118.4 ms. Gates remain guest >=50,
      host >=55, frame <=50 ms and audio >=40 ms, with 1000 ms warmup. Actual
      archive and page module HTTP bodies match pinned sizes and SHA-256.
      The new PowerPC worker uses a cached module without a separate HTTP body
      receipt. Its actual loaded Wasm wire bytes are captured and independently
      match the same released 15854884-byte module, SHA-256
      eee14504e744db7a2dc780f968cc2a5a9ee6f7c0fabf601d498a48b2df750c55.
      The debugger is disabled before measurement; no disassembly, guest tracing,
      breakpoint or mutation is used.
      All targets resumed, with zero lifecycle/observer errors and no concurrent
      heavy job. Earlier readiness and observer setup failures are retained in
      the issue; no unchanged measured pacing failure was retried. Complete
      levels/campaign, combat accuracy, saves, multiplayer, independent visual
      parity and audio fidelity remain unverified.
controls:
  arrows_as_numpad: true
  mobile:
    buttons:
    - key: Space
      label: A
    - key: Tab
      label: B
    - key: Enter
      label: C
    enabled: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 3f497f18a225632c4c4166f9d2faddefce3ddd36b932ca13425eb92266c4c62a
    size_bytes: 27112342
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://trilogyrelease.bungie.org/
    - https://trilogyrelease.bungie.org/faq.html
    - https://github.com/benletchford/systemless/issues/5302
    rights_holder: Bungie
    permission: >-
      Bungie authorizes free distribution of the original Marathon trilogy; copyright
      is retained and commercial sale is not permitted.
    notes: >-
      Original archive linked from the Trilogy Release page, 27,112,342 bytes,
      SHA-256 3f497f18a225632c4c4166f9d2faddefce3ddd36b932ca13425eb92266c4c62a. The
      previously hosted package differs in resource forks and Finder metadata; its exact
      provenance has not been established.
- id: screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 12a594c113e98aef28a58e3b050ab6a58713422cd18e1492e25ac17f559f11ee
    size_bytes: 143662
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5302
    permission: >-
      Original gameplay screenshot captured for this catalogue at the maintainer's
      request.
    notes: >-
      Captured from the original Trilogy Release archive during verified native 68K
      movement, cropped to the game content surface.
references:
- https://trilogyrelease.bungie.org/
- https://github.com/benletchford/systemless/issues/5302
---

## Source and version

This entry tracks the original [Trilogy Release archive](https://assets.systemless.org/catalogue/objects/sha256/3f/3f497f18a225632c4c4166f9d2faddefce3ddd36b932ca13425eb92266c4c62a.sit). Released-browser 68K and PowerPC tests verify first-level initialization and forward movement, with omitted-input controls and separate sustained gameplay. All eight twenty-second pacing windows pass on v0.112.0. Full levels, combat accuracy, saves, multiplayer and audio fidelity remain unverified.

## Across Lh'owon

![Marathon 2 gameplay in the first level](https://assets.systemless.org/catalogue/media/sha256/12/12a594c113e98aef28a58e3b050ab6a58713422cd18e1492e25ac17f559f11ee.png)

Seventeen years after Tau Ceti, Durandal wakes you over the S'pht homeworld of Lh'owon and sends you hunting through its ruins. The wider spaces, water, weather and dual-wielded weapons make the action livelier, while terminals piece together a history of enslavement that gives every ruined complex a purpose.
