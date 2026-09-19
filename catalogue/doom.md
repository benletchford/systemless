---
id: doom
kind: game
title: Doom
summary: >-
  Push through the corridors of Phobos with a shotgun, a dwindling supply of
  shells and far too many things moving in the dark.
developer: id Software
publisher: id Software
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: FPS
compatibility:
  status: playable
  verified:
  - date: 2026-09-15
    tester: Catalogue maintainer
    systemless_version: 0.41.2
    architecture: 68k
    environment: Deterministic headless framebuffer run from the original BinHex archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/1983
  - date: 2026-09-15
    tester: Catalogue maintainer
    systemless_version: 0.41.2
    architecture: ppc
    environment: Deterministic headless framebuffer run from the original BinHex archive
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/1983
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 61f8b9ee99e0bc45edc098f8490759e0a9503ab2333d8ac735dc2b45a112bbd3
    size_bytes: 3458821
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.gamers.org/pub/idgames/idstuff/doom/mac/MacDoomDemo.hqx
    - https://www.gamers.org/pub/idgames/idstuff/doom/mac/MacDoomDemo.txt
    - https://www.gamers.org/pub/idgames/historic/
    - https://www.gamers.org/docs/FAQ/doomfaq/sect1.html
    - https://doom.bethesda.net/en-US/doom_doomii
    license: id Software Doom shareware distribution
    rights_holder: id Software LLC, published by Bethesda Softworks
    permission: >-
      The idgames record identifies this file as the official Mac demo distribution.
      The archive retains actual id Software Internet distributions with id's
      permission, and id's official Doom FAQ records the shareware release as legal wherever
      obtained.
    notes: >-
      This unchanged 3,458,821-byte BinHex file was retrieved independently from
      idgames on 2026-09-15. It has SHA-256
      61f8b9ee99e0bc45edc098f8490759e0a9503ab2333d8ac735dc2b45a112bbd3 and contains the original application, Doom Read Me, Help Me
      document, Troubleshooting document, DOOM1.WAD and music folder. The current rights
      holder also sells the separate, modern DOOM + DOOM II release; this artifact is
      only the historical Mac shareware demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 163e30c05957f468659c2d2591f1c235ef31557eb5c587a7d4e182406d61092d
    size_bytes: 69463
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/145
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh attract-mode gameplay capture made from the unchanged archive on
      2026-09-15. The 800x600 framebuffer was cropped to the 640x400 game surface, removing
      only unrelated black margins; no game pixels were altered.
references:
- https://www.gamers.org/pub/idgames/idstuff/doom/mac/MacDoomDemo.txt
- https://www.gamers.org/docs/FAQ/doomfaq/sect1.html
- https://doom.bethesda.net/en-US/doom_doomii
---

## One room is never just one room

![Doom gameplay](https://assets.systemless.org/catalogue/media/sha256/16/163e30c05957f468659c2d2591f1c235ef31557eb5c587a7d4e182406d61092d.png)

A corridor opens into a storage bay. There is a shotgun on the floor, an imp at
the far wall and just enough space to believe the situation is under control.
Then a side passage moves. Doom builds its rhythm from these small betrayals:
doors conceal crossfire, useful supplies draw the eye away from danger, and a
retreat can uncover something that was quiet a moment ago.

The view stays deliberately spare. A face in the status bar winces as health
falls; ammunition numbers make every missed shot legible. With no quest log or
map marker competing for attention, the geography itself becomes the puzzle.
The fastest route is rarely the safest one, and the safest one still has teeth.

## The whole first episode

The Macintosh demo carries all nine maps of *Knee-Deep in the Dead*, rather
than a short timed sample. It also carries its own music files and the
shareware `DOOM1.WAD`, so the complete original BinHex package matters: those
pieces belong together, and Systemless discovers them after opening the archive.

This is the 1995 Macintosh conversion by Lion Entertainment, not a browser
remake and not the modern commercial edition. Its application contains both
68k and PowerPC code. The catalogue starts with the 68k path, which reaches the
game directly from the untouched shareware distribution.
