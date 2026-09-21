---
id: troubled-souls
kind: game
title: Troubled Souls
summary: Save an alchemist's soul in Randy Reddig's gothic arcade-puzzle demo.
developer: Randy Reddig
publisher: Varcon Systems, Inc. / MacSoft
year: 1994
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
      Deterministic headless gameplay run under Systemless from the exact unchanged
      68k StuffIt archive; the live board and score/control UI were reached before input
      returned to the game launcher.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2335
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 58445790033e0e52ea20b3c9a86c84182cbeaa99fe3731fd013d4a38634cbbd0
    size_bytes: 1303848
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.vintageapplemac.com/software/games/t/
    - https://www.vintageapplemac.com/files/games/Troubled%20Souls.sit
    - https://static.classicmacdemos.com/demos/troubled-souls/README.txt
    license: Varcon Systems demo distribution permission
    rights_holder: Randy Reddig and Varcon Systems, Inc.
    permission: >-
      The included demo ReadMe says the demo is freely distributable as long as that
      ReadMe remains included. This archive contains the notice.
    notes: >-
      Unchanged 1,303,848-byte demo archive from VintageAppleMac, SHA-256
      58445790033e0e52ea20b3c9a86c84182cbeaa99fe3731fd013d4a38634cbbd0.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c5519e93e1118d5a2d6444e7628ed0218edc654d471f220e993f604443a3fd91
    size_bytes: 206194
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2335
    - https://www.vintageapplemac.com/files/games/Troubled%20Souls.sit
    - https://static.classicmacdemos.com/demos/troubled-souls/README.txt
    permission: >-
      Original gameplay screenshot captured from the exact unchanged shareware
      archive for this catalogue entry. Underlying game artwork remains the property of Randy
      Reddig and Varcon Systems, Inc.
    notes: >-
      Fresh deterministic Systemless 0.45.0 capture on 2026-09-21 using the exact
      1,303,848-byte archive above. The run reached the live board with score and control
      UI, and the scripted mouse input transitioned back to the launcher; the 800×600
      framebuffer was cropped to the 512×384 game surface, excluding the black emulator
      margins. PNG SHA-256
      c5519e93e1118d5a2d6444e7628ed0218edc654d471f220e993f604443a3fd91; size 206,194 bytes.
references:
- https://classicmacdemos.com/troubled-souls
- https://www.vintageapplemac.com/software/games/t/
---

## A gothic puzzle under pressure

Troubled Souls asks the player to reason quickly through an alchemist's strange
predicament, backed by Randy Reddig's art and Jim Holt's music. The complete
freely distributable demo comes from VintageAppleMac. Systemless compatibility
was verified by a deterministic run that reached the live board and accepted
mouse input.

![Troubled Souls gameplay](https://assets.systemless.org/catalogue/media/sha256/c5/c5519e93e1118d5a2d6444e7628ed0218edc654d471f220e993f604443a3fd91.png)
