---
id: return-to-zork
kind: game
title: Return to Zork
summary: Visit the Valley of the Vultures in Activision's original Macintosh demo.
developer: Activision
publisher: Activision
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: boots
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: 0.51.0 + local runtime
    architecture: 68k
    environment: >-
      Deterministic 800-by-600, 256-colour run of the unchanged Macintosh demo ZIP.
      Dismissed its CD-ROM prompt, watched the Valley of the Vultures intro, and clicked
      the travel cursor to reach the Lighthouse scene. Browser launch remains disabled
      until the promoted archive is tested on the site.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2671
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: e9d2540fb99ca820b8cb17d485eba8878eae81b553c097d0b56133c057c35a09
    size_bytes: 142911751
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.scummvm.org/demos/
    rights_holder: Activision, Inc. and successors
    permission: >-
      ScummVM publicly preserves this separately identified Macintosh demo. It
      contains a 68K MacBinary application and a limited promotional data set, not the retail
      Macintosh CD or one of the DOS demos. No express redistribution licence was
      found inside the ZIP.
    notes: >-
      Unchanged 142,911,751-byte ZIP from ScummVM's demo collection, SHA-256
      e9d2540fb99ca820b8cb17d485eba8878eae81b553c097d0b56133c057c35a09. ZIP integrity passes.
      The MacBinary application has nine CODE resources and no PPC-only cfrg resource.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b9212c6e4ca98701ef6447a7c5fcac6bdd14615753741a37bf16cfcdd2c578b6
    size_bytes: 206702
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2671
    permission: >-
      Fresh gameplay capture from the unchanged Macintosh demo. The underlying
      artwork remains the property of its rights holders.
    notes: >-
      Cropped the native 640-by-480 game content from the centre of an 800-by-600
      Systemless framebuffer after travelling to the Lighthouse. The crop removes only
      black border pixels and changes no game pixels.
references:
- https://www.scummvm.org/demos/
---

## Into the Valley of the Vultures

![The lighthouse reached in the Return to Zork Macintosh demo](https://assets.systemless.org/catalogue/media/sha256/b9/b9212c6e4ca98701ef6447a7c5fcac6bdd14615753741a37bf16cfcdd2c578b6.png)

This is the original Macintosh promotional demo preserved by ScummVM, not the
commercial CD or a DOS edition. After the introduction, point at a red travel
cursor and click to move between scenes. The first route reaches the
Lighthouse.

The classic application may ask for its CD even though the demo data is
included. Press OK to continue. Browser launch awaits testing of the promoted
archive.
