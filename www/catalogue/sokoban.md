---
id: sokoban
kind: game
title: Sokoban 2.4.2a
summary: >-
  Push treasure chests onto their destinations in Scott Lindhurst's warehouse
  puzzle.
developer: Scott Lindhurst
year: 1999
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: 357f79d1ac105f18e2bfbbc37c7902c7f6ca684e native candidate
    architecture: 68k
    environment: >-
      Complete unchanged 2.4.2a archive, exact game selector, explicit 68K and
      800x600/depth 8. Return advances a startup notice with a blank body to the new-player
      dialog. Ordinary typing creates Player. Up and five Left moves approach a chest; a
      final Left pushes it one cell, changing Moves 6/Pushes 0 to Moves 7/Pushes 1.
      The equal-duration control omits only the last input and leaves the chest in place.
      Fresh active/control repeats each pass six measured pixels and six exact prior
      Systemless captures, 12 checks at 437 frontend / 1037 guest ticks, zero
      exhaustion. Repeated captures are not hardware oracle evidence. Native binary SHA-256
      a5c1aefb540466866a43374b6be4e54acd51e741c9eb953827b1f895d3805eb0. Production packing
      independently matches all twelve original paths, twenty-four forks and twelve
      supported Finder records against unar, with strict end validation. Runtime source is
      identical to merged 2a0e56f6. Browser input/pacing, completed levels,
      startup-notice fidelity, persistence and audio remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5711
  - date: "2026-11-07"
    tester: Catalogue maintainer
    systemless_version: ada7ad31c73e11df98de9f7e8036ccc8bf4f69c8 private browser candidate
    architecture: 68k
    environment: >-
      Private frontend on merged runtime 2a0e56f6. Chromium 156.0.8078.79/Linux
      aarch64, worker/WebGL, 25 MHz. Private HTTPS fixture matches deployed COEP
      credentialless/COOP same-origin; isolation and SharedArrayBuffer verified.
      Actual hosted archive request is unintercepted and its 144881-byte browser
      response body matches the original SHA-256. Main/worker Wasm response bodies
      both match 17755626 bytes and SHA-256
      25b1cdd1e6e97ecd554ad0b52bc753d03ffab985cc5224ba4512b66437f72efb.
      Trusted typing creates Player; Up/five Left reaches a chest and a final Left
      pushes it, changing Moves 6/Pushes 0 to Moves 7/Pushes 1. Four further moves
      occur during a 20-second sample. Final steady host/guest rates 60.014 per
      second, maximum frame 88.2 ms, minimum audio queue 73.2 ms, zero captured
      lifecycle/network errors. The unchanged 50 ms frame gate fails; no launch
      approval. Original monochrome configuration in private source 1950e948 also
      fails at 123.5 ms maximum frame and 19.9 ms minimum audio queue. Source keeps
      the color configuration disabled. Initial idle-desktop setup samples are
      excluded from gameplay evidence. Completed levels, notice fidelity, saves,
      audio fidelity and released public play remain unverified.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5711
runtime:
  executable_path: Sokoban 2.4.2a/Sokoban 2.4.2a
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 8070a88266b8af34304cf1799a8ee9b873c0f56571074480c72e49d798bb352c
    size_bytes: 144881
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/files/games/Sokoban%202.4.2a.sit
    - https://github.com/benletchford/systemless/issues/5711
    license: Original online-distribution grant
    rights_holder: Scott Lindhurst
    permission: >-
      Original Sokoban docs explicitly permit online distribution by online services
      or BBSs provided this documentation remains included unchanged. The retained
      About artwork independently permits distribution via online services. CD-ROM
      distribution has a separate reference-copy condition. Preserve the complete original
      package, documentation and notices, including the authorized twenty-level Sokowahn
      free sample and original paid-level ordering conditions. No paid levels are
      unlocked.
    notes: "Original 23877-byte docs SHA-256 b9c1e61f1cfdd0ae838a5881a25d0994459411add6fe88b9939804b9f978a3a3. All retained data/resource notices were reviewed before promotion. Version 2.4.2 documents removal of disputed Boxxle and IQ Carrier levels. Actual game resource fork is 139567 bytes, SHA-256 5d78bd692a739a3870e15196780511d5b284d0698f1e3a87019cd3154c4842cc, with CODE 0-4 and no cfrg: 68K, independently of the bundled font file."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2180f22696ae9bf3e1606bdfa4cfb770d6e27be3381456e37efe3286b768fb12
    size_bytes: 4224
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5711
    permission: Original gameplay capture from the permitted unchanged distribution.
    notes: >-
      Native 357f79d1 after a legal chest push. Pixel-exact 496x300 content/HUD crop
      at (151,90), without resampling; excludes menu bar, desktop and window framing.
references:
- https://github.com/benletchford/systemless/issues/5711
---

![Sokoban after a legal chest push](https://assets.systemless.org/catalogue/media/sha256/21/2180f22696ae9bf3e1606bdfa4cfb770d6e27be3381456e37efe3286b768fb12.png)

Enter a player name when prompted and accept. If a startup notice appears
first, press **Return** to continue. Use arrow keys or click adjacent squares to move. Push each treasure chest onto a destination;
chests cannot be pulled. **Escape** undoes a move; **Space** redoes it.

The bounded native replay verifies movement and a chest push against an
omitted-input control. Hosted-browser input also moves and pushes, but movement
redraws exceed the pacing limit; launch stays disabled. Completed levels,
startup-notice fidelity, saved progress, audio fidelity and released public play
remain unverified.
