---
id: flashback-the-quest-for-identity
kind: game
title: "Flashback: The Quest for Identity"
summary: Guide Conrad through the jungle in MacPlay's original Macintosh demo.
developer: Delphine Software
publisher: MacPlay
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "0.56.0"
    architecture: 68k
    environment: >-
      Deterministic run of the unchanged 68K Flashback Demo archive at 800 by 600.
      The publisher splash and title menu render. Space selects Start, and a later key
      press skips the opening cinematic to the first jungle scene. Holding Right moves
      Conrad across the scene. Browser launch has not yet been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2610
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: e42ec37ae9659c97e81fd398a1e75ad5a92b4179c4207d3063fbb6e615b1fa99
    size_bytes: 2388806
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/flashback-the-quest-for-identity
    - https://coverdiscs.com/disc/the-mac-12
    rights_holder: Delphine Software, MacPlay/Interplay, and successors
    permission: >-
      This is the unchanged, intentionally limited MacPlay promotional demo, not the
      retail game. The archive contains an application named Flashback Demo, whose own
      resources identify demo content. A contemporary Macintosh cover disc carried the
      demo. The archive has no express redistribution licence.
    notes: >-
      Original 2,388,806-byte StuffIt 5 archive, SHA-256
      e42ec37ae9659c97e81fd398a1e75ad5a92b4179c4207d3063fbb6e615b1fa99. The application in the archive is dated
      1995-02-12.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 073e0148939e5a77aa9196d0588f2d2cd71b662f3a3b2bcc4c950ee57b93967e
    size_bytes: 62024
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2610
    permission: >-
      Fresh deterministic gameplay capture made for this catalogue from the unchanged
      demo. Underlying game artwork remains the property of its rights holders.
    notes: >-
      Captured after holding Right in the first jungle scene. The 512-by-450 game
      content rectangle at (144,86) was cropped from the 800-by-600 framebuffer without
      altering game pixels. PNG SHA-256
      073e0148939e5a77aa9196d0588f2d2cd71b662f3a3b2bcc4c950ee57b93967e, 62,024 bytes.
references:
- https://classicmacdemos.com/flashback-the-quest-for-identity
- https://github.com/benletchford/systemless/issues/2610
---

## The jungle opening

![Conrad in the Flashback Macintosh demo's jungle](https://assets.systemless.org/catalogue/media/sha256/07/073e0148939e5a77aa9196d0588f2d2cd71b662f3a3b2bcc4c950ee57b93967e.png)

Conrad wakes in an alien jungle and must find a way through its platforms and
hazards. This original Macintosh demo presents the opening sequence and a
limited portion of the game, rather than the commercial release.

Systemless reaches the title menu and first jungle scene. Press any key to
leave the publisher splash, select Start, then press a key during the opening
cinematic to skip ahead. Use the arrow keys to move; Shift is the action key.
Browser launch awaits manual approval.
