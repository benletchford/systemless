---
id: sideminder
launch_enabled: true
kind: game
title: SideMinder! 1.1
summary: Match colored tile edges against the computer and empty your pile first.
developer: John V. Holder
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: >-
      Native runtime/build code equals merged master 05e33193.
      Complete unchanged original, canonical executable, explicit 68K/default 25
      MHz/800x600/depth 8 and original settings. Startup reaches the empty board with 35
      pieces each and zero scores. Ordinary board click places a player tile and elicits
      a computer reply; both counters become 34 and computer score becomes 15. Matched
      control performs the same pointer movement without a button, retaining the empty
      board, 35/35 pieces and zero scores. Before-input captures match exactly. Both
      fresh repeats pass eight measured causal pixel assertions and two exact earlier
      captures, ten checks each at 304 frontend/904 guest ticks with zero exhaustion.
      Actual images inspected. Own-run references establish consistency, not hardware
      accuracy. Native binary SHA-256
      35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34. Full matches, tournament play, save/load, sound fidelity, browser
      pacing and released public play remain unverified. Launch stays disabled pending
      browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5770
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 6d00c89e01e2ad3d493d98e6bbada54847863fc1 private browser on merged 05e33193 runtime
    architecture: 68k
    environment: >-
      Fresh isolated Chrome worker/WebGL active and omitted-button sessions use the
      complete unchanged original, canonical executable, original settings and
      68K/default 25 MHz/800x600/depth 8. The same ordinary blank-canvas click before
      readiness/measurement supplies an audio user gesture in both sessions. Each
      measures 20 seconds of ordinary board interaction. Two player placements and
      computer replies leave four board tiles, 33 pieces each, player score 15 and
      computer score 30. Control moves the pointer without placement buttons and
      retains the empty board, 35/35 pieces and zero scores. Actual images inspected.
      Active guest/host rates are 60.00/60.00 per second, maximum worker frame
      43.8 ms and minimum audio queue 122.6 ms; control rates are 60.00/60.00,
      maximum frame 4.0 ms and minimum audio 132.7 ms. Both pass unchanged guest
      >=50/sec, host >=55 FPS, worker <=50 ms and audio >=40 ms gates with no
      lifecycle/network errors. Actual game ID, worker mode, architecture, clock,
      isolation and shared memory are verified. Original archive and both main/worker
      Wasm HTTP 200 response bodies match independent byte/hash pins. An earlier
      omitted-button attempt lacked an audio user gesture and produced no audio
      diagnostics; it was rejected rather than relaxing the gate. Full matches,
      tournament play, save/load, sound fidelity and released public play remain
      unverified. Public replay awaits a human release.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5770
runtime:
  executable_path: SideMinder! Folder/SideMinder! 1.1
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: b8de7f5027976e74889457b9ea691017f10f1b9f71300993c2a3d043f8a6c7d6
    size_bytes: 143223
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/d/sw/games/sidemind/c/SideMinder1.1.sea.hqx
    - https://www.gryphel.com/c/sw/games/sidemind/
    - https://github.com/benletchford/systemless/issues/5770
    license: Original complete-package upload terms
    rights_holder: John V. Holder
    permission: >-
      The original Help Manual Distribution section permits uploads to any BBS
      provided both documents accompany the game. CD distribution additionally requires
      sending the author a copy; no CD distribution is performed here. Preserve the complete
      original, both documents, the registration application, copyright and original
      $10 shareware request. Registration is not supplied or bypassed. Both Read Me and
      Help Manual were read in full.
    notes: >-
      Independent unar/lsar versus unchanged production packing verifies four
      original paths, eight forks and four Finder records, strict KPK end 176053, pack SHA-256
      d9d5545d0487ed3b143950a09bc45cc90e30826769526c3b3521a5922502c022. The
      extractor's containing directory original is a synthetic host wrapper; all original inner
      paths and bytes are retained. The host's published MD5 matches the original
      archive. Resource-only game has CODE 0/1/2/3 and no cfrg/PPC slice. Registration
      application is preserved but not launched.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0fbbf0da5557abc8abb53c7838817a336c96afe14947d2f0b23580dbb93a0770
    size_bytes: 2400
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5770
    rights_holder: John V. Holder
    permission: Fresh gameplay capture from the unchanged distributable original.
    notes: >-
      Exact 504x259 native content crop at (148,191), excluding the Classic menu bar,
      window title and frame; no resampling. Shows the player placement and computer
      reply, both remaining-piece counters and scores. Not a hardware reference.
references:
- https://www.gryphel.com/c/sw/games/sidemind/
- https://github.com/benletchford/systemless/issues/5770
---

![SideMinder player tile and computer reply](https://assets.systemless.org/catalogue/media/sha256/0f/0fbbf0da5557abc8abb53c7838817a336c96afe14947d2f0b23580dbb93a0770.png)

Click a board space to place your top tile, matching the colored edges of its
neighbors. The computer replies after your move. Click your pile to draw when
no legal placement is available. When rotation is enabled, press Space to rotate
your tile; rotation costs points. The original Help Manual explains scoring and
tournament play.
