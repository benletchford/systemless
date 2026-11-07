---
id: domain
kind: game
title: Domain
summary: >-
  Clone or jump pieces across the board, capturing adjacent opponents to claim
  the larger domain.
developer: Mike Perry
year: 1998
architectures:
- 68k
- ppc
default_architecture: 68k
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: "Complete unchanged original, exact executable, 68K/default 25 MHz/800x600/depth 8. Dismiss title, New Game, acknowledge computer-first dialog. Select red (212,150), adjacent (266,150): red clones and computer replies. Equal-clock omitted-destination control keeps original position. Repeats each pass eight causal pixels plus two exact prior captures at frontend 860/guest 1460, zero exhausted frames. Actual captures inspected; own-run repeatability is not hardware accuracy. Known limitation: counters remain User 2/Computer 3 while board has 3 red/4 green. Guest DrawString requests 3/4 but retained board clipping excludes counters; cause remains unproven. No compensation or game alteration. Binary SHA-256 35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34. Full games, final scoring, alternate boards/difficulty, audio fidelity and browser pacing remain unverified. Launch disabled pending browser qualification."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5790
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: ppc
    environment: >-
      Original fat PowerPC slice, default depth. Halts at unsupported
      InterfaceLib:InitPack before completing the first frontend tick. No gameplay. Use 68K.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/5790
runtime:
  executable_path: Domain 1.0/Domain (FAT)
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: a9ffd119dfe354b271396c8c34e438ba3a2d07e20e24375216810a2431b744bc
    size_bytes: 457321
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/domain-10.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/00game-abstracts.txt
    - https://github.com/benletchford/systemless/issues/5790
    license: Original unchanged distribution terms
    rights_holder: Mike Perry
    permission: >-
      Complete original HTML instructions explicitly permit copying in original form
      with all instructions, free of charge. Postcardware; preserve the whole original
      archive, HTML instructions, instructional JPEGs, forks and notices. Full
      2563-byte instructions read.
    notes: "Independent extraction matches production packing: five paths, ten forks, five Finder records, strict KPK end 744892, pack SHA-256 0234febf825b6d0995da1aefe59ee146df9840804098c5e5064a8fa2bfc02512. Original StuffIt-in-BinHex, fat 68K/PowerPC executable; original copyright identifies 1998. Only 68K gameplay qualified; PPC halts at InitPack."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f8fb5347bfd97d13e483c221e3c422305e5151b78135d8af6b415993de606cbf
    size_bytes: 35680
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5790
    rights_holder: Mike Perry
    permission: Fresh content-only capture of the unchanged distributable original.
    notes: >-
      Exact crop (82,121)-(718,559), 636x438, excluding window chrome, no resampling.
      Shows ordinary clone and reply, including the known stale side counters.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/00game-abstracts.txt
- https://github.com/benletchford/systemless/issues/5790
---

Click to dismiss the title, then choose New Game from the File menu. Select one of your red pieces, then a destination: an adjacent square clones the piece; a square two spaces away jumps it. Enemy pieces beside the destination become yours.

**Known limitation:** the side counters can stay stale after moves. The board shows the changed pieces; final scoring has not been verified. Use the 68K version; the PowerPC version currently stops during startup.
