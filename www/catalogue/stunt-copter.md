---
id: stunt-copter
kind: game
launch_enabled: false
title: StuntCopter 1.2 (Clouds)
summary: Fly a helicopter and time a stuntman's drop into a horse-drawn cart.
developer: Duane Blehm
publisher: HomeTown Software
year: 1986
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-10-05"
    tester: Catalogue maintainer
    systemless_version: 0.76.1 + deterministic play runner
    architecture: 68k
    environment: >-
      The original resource-only 1.2 (Clouds) application was repacked into a
      MacBinary container without changing its resource fork. The application
      opens, Begin enters the animated helicopter and cart playfield, and a
      mouse click advances the attempt display. The same corrected Begin input
      reaches the playfield in BasiliskII. A successful cart landing and save
      behaviour remain unverified. Browser validation is pending.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/4041
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: incoming
    path: catalogue/incoming/stunt-copter/stunt-copter-1.2.bin
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mace.home.blog/files/
    - https://groups.google.com/g/comp.sys.mac.games/c/g8vTz3UN6y4
    permission: >-
      A January 1995 discussion reports that Duane Blehm's parents placed
      StuntCopter in the public domain. M.A.C.E. documents its bundled game as
      an unmodified original 68k version and likewise identifies the public-domain
      release.
    notes: >-
      This 28,800-byte MacBinary container was made from M.A.C.E.'s original
      application data and resource forks. The 28,607-byte game resource fork is
      unchanged (SHA-256 a144fc53aa3eeb780febd749c3eddafdd933853bbc0d2d4c6cff7492d3b28feb).
      The container SHA-256 is 2027cd01ea8e3ce62fb7945cd1bbfdcc1123c2954374644bf3a9d3ad693edabc.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/stunt-copter/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/4041
    permission: >-
      Fresh Systemless gameplay capture of the original public-domain game.
    notes: >-
      Cropped 512×325 playfield after Begin, excluding the Mac menu bar and
      emulator framing. SHA-256 9228e4a5d451921067958e5d5732855e29a8e5ac4ae9857cd01ceb8a45a7ce76.
references:
- https://mace.home.blog/files/
- https://groups.google.com/g/comp.sys.mac.games/c/g8vTz3UN6y4
---

## Time the drop

![StuntCopter's helicopter and cart playfield](incoming/stunt-copter/gameplay.png)

Choose **Begin** to start. Move the mouse to position the helicopter, then click
as the cart passes beneath it to drop the stuntman.

This entry uses Duane Blehm's original 68k game, released to the public domain
by his family. The application resource fork is unchanged; its MacBinary
container supplies the file metadata needed by the browser runtime.
