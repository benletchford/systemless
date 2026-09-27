---
id: spectre-supreme-demo
kind: game
title: Spectre Supreme Demo
summary: >-
  Enter Velocity's first-person tank arena in its original Macintosh
  demonstration.
developer: Velocity Development
publisher: Velocity Development
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: boots
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 111d69e6e6ad546fad81c3da0797933d257bd6c8
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged demo. The main menu opened, Play
      led to tank selection, and the default tank entered a live level. The timer and
      arena scene changed over successive captures. Steering, combat, native-Mac
      comparison, and browser launch have not yet been verified. The promoted
      archive and screenshot were fetched back and matched their recorded
      SHA-256 hashes.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/3082
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 43efbc6de8b6b14cfbacf006d32f73a955d1adfd60d92dd45dbb39665574cbbe
    size_bytes: 667110
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/spectre-supreme
    - https://download.classicmacdemos.com/Spectre%20Supreme%20Demo.sit
    rights_holder: Spectre Supreme rights holders
    permission: >-
      This standalone application identifies itself as Spectre Supreme Demo and
      invites players to order the retail release. Classic Macintosh Game Demos records the
      same promotional title on eleven contemporary software discs. This entry
      preserves the exact unchanged demo archive, not the retail game. No separate Read Me or
      express redistribution restriction is bundled.
    notes: >-
      Original 667,110-byte StuffIt archive, SHA-256
      43efbc6de8b6b14cfbacf006d32f73a955d1adfd60d92dd45dbb39665574cbbe. Contains a single 68K application named Spectre
      Supreme Demo.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: ac7c6bc341dd3a946df3d1d9739af6efbc211b17aefe15cae61f2eb5c76d7fd9
    size_bytes: 5979
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3082
    permission: >-
      Fresh live-level capture made for this catalogue entry from the unchanged
      promotional demo. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 512-by-360 game-content crop at (144,90) of a deterministic 800-by-600
      Systemless framebuffer. The crop excludes surrounding desktop without changing game
      pixels. PNG SHA-256
      ac7c6bc341dd3a946df3d1d9739af6efbc211b17aefe15cae61f2eb5c76d7fd9, 5,979 bytes.
references:
- https://classicmacdemos.com/spectre-supreme
- https://download.classicmacdemos.com/Spectre%20Supreme%20Demo.sit
---

## The first tank run

![Spectre Supreme demo live-level tank view](https://assets.systemless.org/catalogue/media/sha256/ac/ac7c6bc341dd3a946df3d1d9739af6efbc211b17aefe15cae61f2eb5c76d7fd9.png)

The demo opens with a tank choice, then drops into a first-person arena. A green
status display tracks lives, score, damage, ammunition, time, and the current
weapon above the horizon. This capture shows the recharge zone during a live
level, with the timer advancing and the scene changing.

This is Velocity Development's original 68K promotional demo, not the retail
release. Systemless reaches live play from the unchanged archive; steering and
combat have not yet been verified. Browser launch remains disabled until a
manual check is complete.
