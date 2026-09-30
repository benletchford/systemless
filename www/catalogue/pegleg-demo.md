---
id: pegleg-demo
kind: game
title: PegLeg Demo
summary: Defend Earth from alien pirates in this top-down arcade shooter.
developer: Sean Ansorge
publisher: Changeling Software
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 4a8505e2 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo. Clicked New Game and
      reached the live arcade playfield. Moving the browser mouse moved the ship from
      the middle to the upper-left and back. One exact archive fetch. Two 10-second
      gameplay samples measured 55.6 and 55.9 host FPS, 59.0 and 59.8 guest ticks per
      second, maximum measured frames of 25.9 and 25.3 ms, and minimum audio queues of 121
      and 120 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3598
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged StuffIt demo, clicked New Game, reached the live
      playfield, and observed mouse movement change the ship and playfield state.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3598
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 93c88cc8f63d52955258e7e9f4e1d8a802c46f02159680bbbda3d6455d2f29c8
    size_bytes: 602217
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/pegleg
    - https://info-mac.org/viewtopic.php?t=5105
    rights_holder: Changeling Software / Sean Ansorge
    permission: >-
      The limited PegLeg demo was publicly distributed through the Info-Mac archive
      in 1997. This is the demo application, not the retail game. No bundled Read Me or
      additional redistribution restriction is present in this StuffIt archive.
    notes: >-
      Unchanged 602,217-byte StuffIt archive, SHA-256
      93c88cc8f63d52955258e7e9f4e1d8a802c46f02159680bbbda3d6455d2f29c8.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 88d8bb51048389470081b228e80908784defdd7e1156142f9e1e0da8e995c92d
    size_bytes: 11887
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3598
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      643-by-484 direct Chrome capture of the live arcade playfield. The crop
      excludes website framing, Mac desktop, menu bar, and window frame. PNG SHA-256
      88d8bb51048389470081b228e80908784defdd7e1156142f9e1e0da8e995c92d, 11,887 bytes.
references:
- https://classicmacdemos.com/pegleg
- https://info-mac.org/viewtopic.php?t=5105
---

## Defend the Earth

![PegLeg arcade playfield](https://assets.systemless.org/catalogue/media/sha256/88/88d8bb51048389470081b228e80908784defdd7e1156142f9e1e0da8e995c92d.png)

Choose **New Game** on the title screen. Move the mouse to steer your ship
through the arcade playfield.
