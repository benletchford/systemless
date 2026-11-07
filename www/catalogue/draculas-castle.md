---
id: draculas-castle
kind: game
title: Dracula's Castle
summary: >-
  Explore Devils Island, defeat its vampire and escape with the hidden treasure
  in this text adventure.
developer: Al Staffieri Jr.
year: 2004
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
compatibility:
  status: unknown
  verified:
  - date: "2026-11-08"
    tester: Catalogue maintainer
    systemless_version: 1285257185892f593f30d83599def53e7a005a1b native candidate
    architecture: 68k
    environment: >-
      Complete unchanged original and exact executable, 68K/default 25
      MHz/800x600/depth 8. New Game then Space acknowledges introduction. N+Return moves BEACH to
      THIN FOREST; equal-clock omitted-Return control retains BEACH with n unsubmitted.
      Before-input images match. Fresh repeats each pass eight measured causal pixels and
      two exact prior captures, ten checks at frontend 560/guest 1156, zero exhausted
      frames. Captures inspected. Own-run consistency is not hardware accuracy. Native
      binary SHA-256 35cf8fcada95a08951a732ff0d92f0b816c2291aeb1067c6ee84788d94b85f34.
      Full story, combat, inventory, save/load, audio fidelity, browser pacing and
      released public play remain unverified. Launch disabled pending browser qualification.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5783
runtime:
  executable_path: Dracula's Castle folder/Dracula's Castle 2.51
  screen_depth: 8
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: fd49d4a9d13ac7f6e1718ca03ccba76a7f2f0e4dd8c8a591007940551e69b98f
    size_bytes: 271104
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gryphel.com/d/sw/games/draccast/c/DraculasCastle.sit
    - https://www.gryphel.com/c/sw/games/draccast/
    - https://github.com/benletchford/systemless/issues/5783
    license: Original unchanged fee-free distribution terms
    rights_holder: Al Staffieri Jr.
    permission: >-
      Complete original ReadMe explicitly permits any number of copies provided the
      application remains unchanged, distribution is fee-free and the ReadMe accompanies
      it. Preserve the complete original, notices and optional $5 hint request. The
      full 6581-byte ReadMe was read.
    notes: >-
      Independent extraction versus production packing matches both original paths,
      four forks and two Finder records, strict KPK end 455009, pack SHA-256
      c7103dd1d730f40e039f69a93630a66e89f63cfe911ec79ed2057e899876cc01. Published MD5 matches.
      Original classic version 2.51 dated 13 February 2004. Compiled FutureBASIC runtime
      is embedded; no external runtime supplied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 2965e124b999559831770850ddcd843a7c75f286651b7faa5507a5994d4cfd9f
    size_bytes: 6452
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5783
    rights_holder: Al Staffieri Jr.
    permission: Fresh gameplay capture from the unchanged freely distributable original.
    notes: >-
      Exact native crop (155,44)-(645,338), 490x294, excluding Classic menu/window
      chrome, no resampling. Shows normal northward movement from BEACH to THIN FOREST.
references:
- https://www.gryphel.com/c/sw/games/draccast/
- https://github.com/benletchford/systemless/issues/5783
---

![Moving north into Thin Forest](https://assets.systemless.org/catalogue/media/sha256/29/2965e124b999559831770850ddcd843a7c75f286651b7faa5507a5994d4cfd9f.png)

Choose New Game and press a key to acknowledge the introduction. Type commands and press Return: N moves north. Search for useful items as you explore. The original ReadMe explains the game and its shareware terms.
