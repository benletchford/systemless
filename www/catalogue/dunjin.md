---
id: dunjin
kind: game
title: The Dungeons of Dunjin 4.4
summary: Explore a mysterious forest and search for treasure in a classic text adventure.
developer: Magnus Olsson
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: d9255200a9a892c3de8002fd975dc02802fdbae9 native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original, canonical executable, explicit 68K/default 25
      MHz/800x600/depth 8 and original settings. The native/runtime build code equals
      merged master debae0f3. Startup reads the original database and reaches in forest.
      Typing N and Return changes location to lost in forest, prints the new description
      and increments Moves from 0 to 1. Matched control types N but omits Return,
      retaining the initial location, Moves zero and unsubmitted input. Before-Return images
      match exactly. Fresh canonical-selector repeats each pass eight measured causal
      pixel assertions and two exact earlier captures, ten checks at 252 frontend/852
      guest ticks with zero exhaustion. Both final images inspected. Own-run references
      establish consistency, not hardware accuracy. Further N, LOOK, E and LOOK commands
      print ordinary responses and advance the turn counter. Actual About and INFO
      identify Macintosh version 4.4. Native binary SHA-256
      09d53dffaf8730437e184ea004f80d4b8100d184263665ff43c7143c01916d17. Full cave traversal, winning, save/load,
      sound fidelity, browser pacing and released public play remain unverified. Launch
      stays disabled pending browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5765
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 5157fe65bafd1076d323c4f37c4e6e56c0a93768 private browser on debae0f3 runtime
    architecture: 68k
    environment: >-
      Fresh isolated Chrome worker/WebGL sessions at original/default 25 MHz,
      800x600 and depth 8. Both main and worker Wasm HTTP 200 response bodies and
      complete promoted archive match independently hashed build/original pins.
      Active N, LOOK, E and LOOK with Return print responses and advance Moves to 4;
      matched control types the same letters without Return, retaining in forest,
      Moves zero and unsubmitted input. Actual final images inspected. Each case
      measures 20 seconds of ordinary interaction. Active guest/host rates are
      62.27/59.25 per second, maximum worker frame 64.9 ms and minimum audio queue
      93.3 ms. Control rates are 60.83/57.88, maximum frame 69.5 ms and minimum audio
      89.1 ms. Both fail the unchanged 50 ms worker-frame gate despite passing
      the rate/audio gates, with no lifecycle or network errors. Overall status
      remains unknown and launch disabled pending a runtime fix and fresh passing
      browser qualification. No released public play claimed.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5765
runtime:
  executable_path: Dunjin44.cpt/Dunjin
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 1db877c464b308d51e6198ef419c40d7eb6a059da4190b2a3a67c36e837a7dc1
    size_bytes: 297952
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ifarchive.org/if-archive/games/mac/Dunjin44.cpt.hqx
    - https://ifarchive.org/indexes/if-archive/games/mac/
    - https://github.com/benletchford/systemless/issues/5765
    license: Original complete-package noncommercial copying terms
    rights_holder: Magnus Olsson
    permission: >-
      The original Macintosh 4.4 manual permits copies, sharing with friends, BBS/FTP
      uploads and readable network partitions, provided all files remain together and
      unchanged in the complete archive. No file may be removed or added; sale or lease
      for profit requires permission. Preserve all seven files, all three
      documentation formats, copyright and the original $20 shareware request. Registration/support
      is not supplied or bypassed.
    notes: >-
      Actual About and INFO identify version 4.4, copyright 1993. An earlier 4.30
      beta resource string is retained unchanged alongside the current 4.4 notice and
      manual. Independent unar/lsar versus unchanged production packing verifies seven
      original paths, fourteen forks and seven Finder records, strict KPK end 348245 and
      pack SHA-256 bcfc5cbe54ec8644cb302924f4ce02d1fe2b25bfa615fa2a7d6e5d6f2ba2d05c.
      Production retains the original CompactPro wrapper name Dunjin44.cpt, confirmed by an
      independent no-recursion BinHex decode; unar's containing directory original is
      a synthetic host extraction wrapper. Every inner name and byte is preserved.
      Resource-only application has CODE 0/1/2/3 and no cfrg/PPC slice.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 924476b60cde3515c429f56ffa6f7300cd87f634d2dee8b4b61c897b217bc371
    size_bytes: 4583
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5765
    rights_holder: Magnus Olsson
    permission: Fresh gameplay capture from the unchanged distributable original.
    notes: >-
      Exact 498x294 native content crop at (8,40), excluding the Classic menu bar,
      window title and frame; no resampling. Shows the forest location, turn counter and
      responses to ordinary N, LOOK, E and LOOK commands. Not a hardware reference.
references:
- https://ifarchive.org/indexes/if-archive/games/mac/
- https://github.com/benletchford/systemless/issues/5765
---

![Dunjin forest after ordinary commands](https://assets.systemless.org/catalogue/media/sha256/92/924476b60cde3515c429f56ffa6f7300cd87f634d2dee8b4b61c897b217bc371.png)

Type a command and press Return. Use directions such as NORTH, SOUTH, EAST and
WEST to explore, LOOK to describe the current place, and INFO for the original
registration and distribution information. The supplied HELP file lists more
commands.
