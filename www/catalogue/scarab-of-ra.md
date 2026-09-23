---
id: scarab-of-ra
kind: game
title: Scarab of Ra
summary: Explore a shifting Egyptian pyramid in Rick Holzgrafe's classic Macintosh shareware adventure.
developer: Rick Holzgrafe
publisher: Semicolon Software
year: 1988
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: false
compatibility:
  status: playable
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.54.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the preserved version 1.4 HFS disk image.
      The pyramid interface renders and a movement control changes the
      first-person view. Browser interaction awaits manual approval.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2513
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: url
    url: https://www.gryphel.com/d/sw/games/scarabra/c/Scarab14.zip
    download_page: https://www.gryphel.com/c/sw/games/scarabra/
    expected_sha256: 090a5e2c9549760dcd7b3ae69518df412bd69bc3c6c548b5ef926bfde54b1d39
    expected_size: 92470
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.semicolon.com/old/Scarab.html
    - https://www.gryphel.com/c/sw/games/scarabra/
    rights_holder: Rick Holzgrafe / Semicolon Software
    permission: >-
      The copyright holder's site still offers Scarab of Ra 1.4 as Macintosh
      shareware. Gryphel labels the preserved copy shareware and packages the
      game from the original self-extracting distribution into an HFS disk
      image inside a ZIP. These are the unchanged bytes of Gryphel's upstream
      distributable, not the original author-distributed container; no
      registration data is included.
    notes: >-
      Gryphel's ZIP includes the 819,200-byte HFS disk image and its checksum.
      The unchanged ZIP has SHA-256
      090a5e2c9549760dcd7b3ae69518df412bd69bc3c6c548b5ef926bfde54b1d39.
      The author's original self-extracting BinHex file is separately documented
      in the source links but is not the playable archive used here.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/scarab-of-ra/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2513
    permission: >-
      Fresh Systemless capture from a live shareware game after one movement.
      The underlying game artwork remains the property of its rights holder.
    notes: >-
      The 512-by-324 game-content area excludes the Classic Mac menu bar and
      unused host framebuffer. PNG SHA-256
      a316117b2afaeee08e3685ef4a574fe29a80f055c00dae6cfea26ae1f94c6fb9,
      9,051 bytes.
references:
- https://www.semicolon.com/old/Scarab.html
- https://www.gryphel.com/c/sw/games/scarabra/
---

## Into the pyramid

![Scarab of Ra pyramid gameplay after moving forward](incoming/scarab-of-ra/gameplay.png)

Search the Great Pyramid of Ra for lost artefacts, using the on-screen controls
to explore its changing rooms and keep track of your supplies. Rick Holzgrafe's
original version 1.4 remains available as shareware from Semicolon Software.

This entry uses a preserved HFS disk-image repack of that game so Systemless
can launch it directly. Headless movement works; browser launch awaits manual
verification.
