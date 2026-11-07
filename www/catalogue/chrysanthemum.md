---
id: chrysanthemum
kind: game
title: Chrysanthemum 1.01
summary: Arrange flowers into matching patterns in a timed ikebana puzzle.
developer: Ryan Koopmans
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: '2026-11-07'
    tester: Catalogue maintainer
    systemless_version: 656e05fb879d175a7fba2294858507a5a93e10ee native candidate
    architecture: 68k
    environment: 'Complete unchanged original, exact executable, explicit 68K, 800x600/depth 8. Return,
      ordinary Play and Return enter the first level. The original flower guides and in-field sprites
      render after merged #5740. Default J held for 30 ticks moves the red flower left relative to
      an equal-clock omitted-key control. Fresh active/control repeats each pass six measured pixels
      and six exact prior own-run screenshots at 516 frontend/1104 guest ticks, zero exhausted frames.
      These images also match the preceding graphics-only runtime exactly. Own-run consistency is
      not hardware-oracle accuracy. Native SHA-256 d90e2f517748f967295d6979cb3f24c85d9250b74ca0ad7c814411ecd0193829.
      I matches idle upward motion and is not input evidence. Generic idle-query correction #5744
      reduces the bounded native scenario from 12520297 to 50551 trap calls without changing images
      or guest ticks. Browser qualification follows; full levels, scoring-rule accuracy, persistence,
      audio fidelity and released public play remain unverified.'
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5725
  - date: '2026-11-07'
    tester: Catalogue maintainer
    systemless_version: b94b3089 private browser preview on ea1fe3e7 runtime
    architecture: 68k
    environment: 'Fresh Chromium 156.0.8078.79/Linux aarch64 profiles, isolated worker/WebGL, SharedArrayBuffer,
      default 25 MHz, 800x600/depth 8. Deployed COEP credentialless and COOP same-origin headers;
      actual source stamp, architecture and clock checked. Actual original hosted archive HTTP 200
      response matches 500405 bytes, SHA-256 da7d02e39474339fcc06f11f0329f55d8e4b8c81e7f7e529ab08a8e7aa0a57c1.
      Actual main and worker Wasm HTTP 200 bodies match the clean build, 17839270 bytes, SHA-256 73d84e6b275d5dc1a2bdbafa09de9b282683f351a0d8492609baa56477787aa0.
      Ordinary Return/Play/Return reaches the first level; default J moves a flower. Unchanged 20-second
      active and idle live-level samples fail the guest-progress gate: final steady guest 39.6414/33.5424
      versus required 50 ticks per second. Host 59.9703/59.9697 per second, max sampled worker frames
      22.3/22.4 ms and min audio queues 166.9/152.1 ms satisfy their gates. No captured lifecycle/network
      errors. Snapshots are not full-level averages. That candidate failed approval and stayed launch-disabled
      for #5743; the later verification below supersedes that hold. Scoring, complete levels, persistence,
      sound fidelity, hardware parity and released public play remain unverified.'
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5743
  - date: '2026-11-07'
    tester: Catalogue maintainer
    systemless_version: c9da0c4490d3f12974a3a503e227ff3a49fe5a6b private browser candidate
    architecture: 68k
    environment: 'Fresh Chromium 156.0.8078.79/Linux aarch64 profiles, isolated worker/WebGL, SharedArrayBuffer,
      default 25 MHz, 800x600/depth 8, production COEP credentialless and COOP same-origin. Actual
      source stamp, clock and architecture checked. Runtime 656e05fb includes the generic idle-query
      correction in #5744. Actual hosted original HTTP 200 body matches 500405 bytes/SHA-256 da7d02e39474339fcc06f11f0329f55d8e4b8c81e7f7e529ab08a8e7aa0a57c1.
      Actual main and worker Wasm HTTP 200 bodies match 17837098 bytes/SHA-256 fd2c880a7711ce3ec3ce3eac807dd005a8d56d08b9b46d1e37e2694361bb0ab3.
      Ordinary Return/Play/Return enters the first level. Default J moves the red flower 16 pixels
      left relative to idle; after-input bounds are (377,329..391,343) versus (393,329..407,343).
      Initial browser ready captures are not pixel-matched: the active capture does not yet show the
      red flower. Both final captures retain the running first level. Unchanged 20-second active/idle
      samples pass: final steady guest 60.0190/60.5911 and host 58.0512/60.5911 per second, maximum
      sampled worker frames 48.8/41.4 ms, minimum audio queues 143.1/139.5 ms, zero captured lifecycle/network
      errors. Snapshots, not whole-level averages. Approve bounded default 68K input and pacing after
      #5744 lands. Complete levels, scoring-rule accuracy, other controls, persistence, sound fidelity,
      hardware parity and released public play remain unverified.'
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5725
runtime:
  executable_path: Chrysanthemum 1.01
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: da7d02e39474339fcc06f11f0329f55d8e4b8c81e7f7e529ab08a8e7aa0a57c1
    size_bytes: 500405
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Chrysanthemum%201.01.sit
    - https://github.com/benletchford/systemless/issues/5725
    license: Original About screen permits unmodified distribution
    rights_holder: Ryan Koopmans
    permission: The original About Game screen identifies Ryan Koopmans, copyright 1993, version 1.01,
      and states that unmodified copies may be distributed freely. Preserve the full unchanged original,
      artwork, music, creator credits and notices. Its JOBWARE employment request remains intact;
      no contact or payment is required or performed.
    notes: Independent extraction matches the single original path, both forks and supported Finder
      record against production packing; strict KPK end 781943, pack SHA-256 21484267915bda2aac64ae5cf7cc2cee2017c3626fda57dd8731d4ed71c9c6cb.
      Resource-only application has five CODE resources and no cfrg/native PPC slice. Retain the complete
      original resource fork, including license and credits.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 42ee6817a08fae6b3280af06140127ff146cc36e23c995a2147f3eb07e06385a
    size_bytes: 24171
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5725
    rights_holder: Ryan Koopmans
    permission: Original gameplay capture from the permitted unchanged distribution.
    notes: Exact 512x342 content crop at (144,86), excluding the window frame, Classic menu bar and
      host UI; no resampling. Flower guides, current flower, quota, score and original creator credit
      remain visible. Not a hardware reference.
references:
- https://github.com/benletchford/systemless/issues/5725
- https://github.com/benletchford/systemless/issues/5743
- https://github.com/benletchford/systemless/pull/5744
---

![Chrysanthemum gameplay](https://assets.systemless.org/catalogue/media/sha256/42/42ee6817a08fae6b3280af06140127ff146cc36e23c995a2147f3eb07e06385a.png)

Arrange flowers into the patterns shown at the left of the garden before time runs
out. Complete each level's quota to advance.

Press **Return** at the opening message, choose **Play**, then press **Return** to
start. Default controls are **I/J/K/L** for up/left/down/right, **U** to cycle a
wildflower and **Space** to zoom. Use **Tab** or **Command-P** to pause.
