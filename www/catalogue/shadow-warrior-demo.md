---
id: shadow-warrior-demo
kind: game
title: Shadow Warrior Demo
summary: >-
  Explore four shareware levels in the original Macintosh demo of 3D Realms’
  Build-engine action game.
developer: 3D Realms
publisher: MacSoft
year: 1997
architectures:
- ppc
default_architecture: ppc
category: FPS
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: e0b55663 runtime code
    architecture: ppc
    environment: >-
      Complete unchanged original demo, default PowerPC, CPU, initial display,
      partition and sound. Accept the normal 256-colour Switch prompt; Space through logos
      reaches the original New Game dialog, then Return accepts its default shareware
      episode and Who Wants Wang skill. The first level renders. Original configuration
      maps Up to forward. Holding Up for 60 ticks and releasing for 12 moves the view
      against an equal-duration no-input control, with byte-identical pre-input images.
      Both finish at 1860 frontend/guest ticks with three measured pixel assertions each
      and zero exhausted frames. Actual captures inspected. This verifies startup and
      bounded movement only; combat, collection, completion, networking, persistence,
      audio fidelity, independent hardware rendering and browser input/pacing remain
      unqualified. Frozen native binary SHA-256
      22f46071915eb224a3475359bbf5fd64118fc901fffbc62bf758ec151a10d4b5 has the same runtime code as merged master e0b55663.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5936
runtime:
  executable_path: Shadow Warrior Demo/Shadow Warrior (Demo)
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: dd81d19709ea8c6c94b0a1f7b5aa6cfaa3c5bf628486618db96ac774dff7a10a
    size_bytes: 11881524
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/shadow-warrior
    - https://archive.org/details/macaddict-cd-017
    - https://github.com/benletchford/systemless/issues/5936
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: 3D Realms / MacSoft
    permission: >-
      Accepted under the project promotional-demo policy with explicit maintainer
      authorization on 2026-11-10. Period cover-disc distribution is corroborated, the
      complete original package is preserved, and no explicit redistribution restriction
      was found in inspected applicable notices. This records project-policy acceptance,
      not an affirmative publisher licence grant. All original notices remain intact.
    notes: >-
      Original 11881524-byte SIT, unchanged including original configuration and
      taunts. All 17 files, 34 forks and 17 Finder records match independent unar
      extraction. Strict expanded KPK 28305422 bytes, SHA-256
      84831f6ea4dd5d8c64590448427dbe5232a39229ba8d00bc0b3d10ca9f40eee5. Preserved January 1998 MacAddict 17 original HFS
      catalogue contains this Commercial Demo. All 17 selected files inspected through
      verified HTTP 206 ranges; 16 data forks and all 207 non-folder-icon resource
      payloads match. Raw resource metadata, folder icon and configuration data differ.
      Config differs only in PlayerName (WADE ALBRIGH on disc, KATO in archive) and
      Chickens (2 on disc, 0 in archive); received settings remain untouched. No whole-disc
      or exact package-byte identity is claimed. Both Readmes (3631/7445 bytes), 108
      selected UI/text/version resources, all retained notice matches and 483-file group
      inventory reviewed. Four shareware levels preserved. No performance suggestions,
      partition/display overrides, configuration rewrites, cheats or unlocks applied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: b3ff2d9c4f56ad98e6ea57cfdd162f619ab5fffa54021264c340bef1f58fd462
    size_bytes: 109546
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5936
    rights_holder: 3D Realms / MacSoft
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      underlying artwork retains its original copyright.
    notes: >-
      640x480 content surface with first-person view, sword and original HUD after
      bounded default forward movement; cropped only from native frame at
      (80,60)-(720,540). No menu bar, desktop, resampling, colour changes or artwork edits. SHA-256
      b3ff2d9c4f56ad98e6ea57cfdd162f619ab5fffa54021264c340bef1f58fd462, 109546 bytes.
      Native evidence only.
references:
- https://classicmacdemos.com/shadow-warrior
- https://archive.org/details/macaddict-cd-017
- https://github.com/benletchford/systemless/issues/5936
---

Play the original four-level Macintosh shareware demo of Shadow Warrior,
3D Realms’ Build-engine action game published for Macintosh by MacSoft.

Accept the original **Switch** prompt for 256-colour mode. Press **Space** through
the logos, then accept the original New Game dialog with **Return**. The default
controls use **Up/Down** to move, **Left/Right** to turn, **Control** to fire and
**Space** to open or use objects.

![Shadow Warrior demo gameplay](https://assets.systemless.org/catalogue/media/sha256/b3/b3ff2d9c4f56ad98e6ea57cfdd162f619ab5fffa54021264c340bef1f58fd462.png)

The complete received demo package is preserved unchanged, including its original
configuration, Readmes, network launcher and taunts. Native startup and bounded
forward movement have been checked. Browser launch remains disabled pending
browser controls and pacing approval.
