---
id: quake-iii-arena-demo
kind: game
title: Quake III Arena Demo
summary: id Software's original Power Macintosh arena shooter demonstration.
developer: id Software
publisher: id Software
year: 1999
architectures:
- ppc
default_architecture: ppc
category: FPS
compatibility:
  status: broken
  verified:
  - date: "2026-09-29"
    tester: Catalogue maintainer
    systemless_version: 9bed7bc0e2f594660d713d241f2dece95d1013a4
    architecture: ppc
    environment: >-
      The unchanged MacBinary installer was parsed with the packed VISE catalog
      decoder. Its installed game executable is PPC-only, but the large pak0.pk3 payload
      still fails VISE decompression. Gameplay and browser launch have not been verified.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/3196
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: b30063f6e1c3c715f2e6d3e9cdefb3942f0c573fcc33a1a66a20dbe9326be831
    size_bytes: 48072960
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://github.com/Jason2Brownlee/Quake3OfficialArchive/blob/main/bin/MacQuake3Demo.bin
    - https://static.classicmacdemos.com/demos/quake-iii-arena/Q3A%20EULA.htm
    license: id Software Quake III Arena demo agreement
    rights_holder: id Software, Inc.
    permission: >-
      Section 3 of the agreement bundled inside this unchanged installer permits
      free, noncommercial distribution of complete demo copies when the agreement
      accompanies each copy. This archive retains that agreement.
    notes: >-
      The 48,072,960-byte MacBinary installer has SHA-256
      b30063f6e1c3c715f2e6d3e9cdefb3942f0c573fcc33a1a66a20dbe9326be831. Its installer wrapper uses 68K code; the
      installed MacQuake3 game has a PowerPC PEF data fork and pwpc cfrg resource with
      no 68K CODE resources. The official release archive identifies this as the
      December 15, 1999 Macintosh demo version 1.11.
references:
- https://github.com/Jason2Brownlee/Quake3OfficialArchive
- https://github.com/benletchford/systemless/issues/3196
---

## The original Mac demo

This unchanged demo installer includes the PPC-only game and its original
agreement. Browser launch is disabled until Systemless can unpack the game data
and gameplay has been verified.
