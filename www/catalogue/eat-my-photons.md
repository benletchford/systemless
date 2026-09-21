---
id: eat-my-photons
kind: game
title: Eat My Photons
summary: >-
  Blast through waves of abstract enemies in Eccentric Software's colourful
  arcade demo.
developer: Eccentric Software
publisher: Eccentric Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-21"
    tester: Catalogue maintainer
    systemless_version: "0.45.0"
    architecture: 68k
    environment: Deterministic headless gameplay run from the exact unchanged archive
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2335
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: ea9fd6bd23e06d8318681e0c341766e8a85cc42695fb66cc2d1d58b6ca8e9d79
    size_bytes: 1873536
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/eat-my-photons
    - https://static.classicmacdemos.com/demos/eat-my-photons/README.txt
    license: Eccentric Software demo distribution permission
    rights_holder: Eccentric Software
    permission: >-
      The included README permits free-of-charge distribution when the entire package
      is included and no part is modified.
    notes: >-
      Unchanged 1,873,536-byte demo archive, SHA-256
      ea9fd6bd23e06d8318681e0c341766e8a85cc42695fb66cc2d1d58b6ca8e9d79.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 23d2a43d629ee88413513022e85d005aaa80a59f1ecb5eba678edd7f8e91c4cb
    size_bytes: 39020
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://classicmacdemos.com/eat-my-photons
    - https://static.classicmacdemos.com/demos/eat-my-photons/README.txt
    - https://github.com/benletchford/systemless/issues/2335
    permission: >-
      Original gameplay screenshot captured from the exact unchanged archive for this
      catalogue entry. Underlying game artwork remains the property of its rights
      holders.
    notes: >-
      Fresh deterministic Systemless capture after creating a new game, accepting the
      first mission and reaching the live cockpit/shooter view. The 800x600
      framebuffer was cropped to the 512x384 game surface; host margins and emulator chrome are
      excluded. PNG SHA-256
      23d2a43d629ee88413513022e85d005aaa80a59f1ecb5eba678edd7f8e91c4cb, 39,020 bytes.
references:
- https://classicmacdemos.com/eat-my-photons
---

## Photons everywhere

Eat My Photons is a quick arcade shooter for 68030-era colour Macs, full of
bright targets and screen-filling movement. This is the complete original demo
package. Systemless reaches the live cockpit view after accepting the first
mission in a deterministic gameplay run.

![Eat My Photons gameplay](https://assets.systemless.org/catalogue/media/sha256/23/23d2a43d629ee88413513022e85d005aaa80a59f1ecb5eba678edd7f8e91c4cb.png)
