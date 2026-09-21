---
id: putt-putt-joins-the-parade
kind: game
title: Putt-Putt Joins the Parade
summary: >-
  Help a small purple car prepare for the Cartown parade in Humongous
  Entertainment's demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Puzzle
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: >-
      Deterministic headless Systemless run from the exact unchanged Putt-Putt Joins
      the Parade demo archive, held through tick 8880 after launch
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2335
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: ec0e84dd0de278f6104f7aca9d71e23738745ee519cd9affdc30a4562ca17ce3
    size_bytes: 1141865
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/putt-putt-joins-the-parade
    - https://static.classicmacdemos.com/demos/putt-putt-joins-the-parade/README.txt
    license: Humongous Entertainment demo distribution permission
    rights_holder: Humongous Entertainment
    permission: The included README permits free distribution of the demo in its original form.
    notes: >-
      Unchanged 1,141,865-byte Macintosh demo archive, SHA-256
      ec0e84dd0de278f6104f7aca9d71e23738745ee519cd9affdc30a4562ca17ce3.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 5394dfc1c0587854e0296ed3a000dbf875997a6e161a8917c0698a29cc3c1178
    size_bytes: 53364
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://classicmacdemos.com/putt-putt-joins-the-parade
    - https://static.classicmacdemos.com/demos/putt-putt-joins-the-parade/README.txt
    - https://github.com/benletchford/systemless/issues/2335
    permission: >-
      Original gameplay screenshot captured from the exact unchanged archive for this
      catalogue entry. Underlying game artwork remains the property of Humongous
      Entertainment.
    notes: >-
      Fresh deterministic Systemless capture after the demo advanced from its title
      sequence into the live Cartown scene and remained there through tick 8880. The
      800x600 framebuffer was cropped to the 640x400 game surface, excluding the Mac menu
      bar, window chrome and host desktop. PNG SHA-256
      5394dfc1c0587854e0296ed3a000dbf875997a6e161a8917c0698a29cc3c1178, 53,364 bytes.
references:
- https://classicmacdemos.com/putt-putt-joins-the-parade
---

## A parade needs a plan

Putt-Putt's first adventure turns preparing for a parade into a friendly chain
of conversations and small problems for young players. This is Humongous
Entertainment's original demo package. A deterministic Systemless run reaches
the live Cartown scene from the title sequence and remains stable there.

![Putt-Putt Joins the Parade gameplay](https://assets.systemless.org/catalogue/media/sha256/53/5394dfc1c0587854e0296ed3a000dbf875997a6e161a8917c0698a29cc3c1178.png)
