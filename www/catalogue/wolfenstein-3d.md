---
id: wolfenstein-3d
kind: game
title: "Wolfenstein 3D: First Encounter"
summary: Escape Castle Wolfenstein in id Software's MacPlay shareware demo.
developer: id Software
publisher: MacPlay / Interplay Productions
year: 1994
architectures: [68k]
default_architecture: 68k
category: FPS
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-21
    tester: Catalogue maintainer
    systemless_version: 0.44.0
    architecture: 68k
    environment: Deterministic headless gameplay run from the original First Encounter 1.0.1 StuffIt archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2299
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 99f1f2e1fd1a8e2fe53504ea38ec7017c234033cdbc19228778eb07ba3598256
    size_bytes: 1379613
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/wolfenstein-3d
    - https://download.classicmacdemos.com/Wolfenstein%203D.sit
    - https://static.classicmacdemos.com/demos/wolfenstein-3d/README.txt
    license: MacPlay / Interplay Productions shareware distribution permission
    rights_holder: id Software and Interplay Productions
    permission: >-
      The bundled shareware alert says, “It is freely distributable, and we do
      encourage you to pass along infinite copies to all your friends!” This
      supports unchanged public CDN redistribution of the First Encounter
      shareware archive. The same notice requests registration and describes
      the paid Second Encounter; it does not grant commercial rights or change
      the stated id Software and Interplay copyrights.
    notes: >-
      Unchanged 1,379,613-byte Wolfenstein 3D: First Encounter 1.0.1 StuffIt
      archive from Classic Mac Demos. SHA-256
      99f1f2e1fd1a8e2fe53504ea38ec7017c234033cdbc19228778eb07ba3598256.
references:
- https://classicmacdemos.com/wolfenstein-3d
- https://static.classicmacdemos.com/demos/wolfenstein-3d/README.txt
- https://github.com/benletchford/systemless/issues/2299
---

## First Encounter

This is the original Macintosh 1.0.1 shareware release of *Wolfenstein 3D:
First Encounter*. It contains the first three Castle Wolfenstein levels and
keeps the bundled application, shareware notice and supporting files together
in the unchanged StuffIt archive.

## Verified route

The tested 68k route opens the bundled shareware dialog, then takes a long
post-dialog title fade before the title is fully visible. A short probe can
therefore look blank even though the application is still rendering. After the
fade, Space opens the difficulty-selection dialog and Return starts Level 1.
Holding Up changes the view and Control fires the weapon, verifying movement
and gameplay interaction beyond launch. The deterministic checkpoints and
framebuffer evidence are documented in [issue #2299](https://github.com/benletchford/systemless/issues/2299).
