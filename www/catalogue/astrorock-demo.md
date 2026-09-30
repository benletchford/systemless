---
id: astrorock-demo
kind: game
title: AstroRock Demo
summary: Pilot a spacecraft through Logicware's colorful arcade shooter.
developer: Logicware
publisher: Logicware
year: 1996
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
    systemless_version: 80ebff8 + local catalogue preview
    architecture: 68k
    environment: >-
      Release-mode Chrome preview of the unchanged StuffIt demo. Chose Start Game,
      focused the game canvas, and pressed Return to enter the live playfield. Browser X
      input changed the ship's orientation. One exact archive fetch. A three-second
      active-state sample measured 60.0 host FPS and 60.3 guest ticks per second, maximum
      measured frame 21.3 ms, and minimum audio queue 266 ms.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3613
  - date: "2026-10-01"
    tester: Catalogue maintainer
    systemless_version: 0.70.1 + deterministic play runner
    architecture: 68k
    environment: >-
      Replayed the unchanged 68K StuffIt demo. Return selected Start Game and cleared
      the playfield prompt. The ship, HUD, and asteroid appeared in the live
      playfield; holding the Read Me's X key visibly rotated the ship.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3613
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: e8cfa21cb38fdacb70e5eeb53b13b3ef95204e0f6585b57596bb5548da568947
    size_bytes: 6271214
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/astrorock
    - https://static.classicmacdemos.com/demos/astrorock/README.txt
    rights_holder: Logicware
    permission: >-
      The unchanged archive contains Logicware's purpose-built 1.0.1 demo and its
      Read Me, which describes the four-level demo and its controls. The bundled material
      contains no additional redistribution restriction.
    notes: >-
      Unchanged 6,271,214-byte StuffIt archive, SHA-256
      e8cfa21cb38fdacb70e5eeb53b13b3ef95204e0f6585b57596bb5548da568947.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 40d106b2074b466a45beb5961764137300e87420b577f83943652642b6e5ce40
    size_bytes: 37812
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3613
    permission: >-
      Fresh gameplay capture made from the original demonstration archive for this
      catalogue entry. The underlying game artwork remains its owners' property.
    notes: >-
      640-by-478 direct Chrome capture of the active playfield, without website
      framing or Mac desktop. PNG SHA-256
      40d106b2074b466a45beb5961764137300e87420b577f83943652642b6e5ce40, 37,812 bytes.
references:
- https://classicmacdemos.com/astrorock
- https://static.classicmacdemos.com/demos/astrorock/README.txt
---

## Turn and fire

![AstroRock playfield](https://assets.systemless.org/catalogue/media/sha256/40/40d106b2074b466a45beb5961764137300e87420b577f83943652642b6e5ce40.png)

Choose **Start Game**, focus the game, then press Return at the playfield prompt.
Use Z and X to turn, N to thrust, M to fire, and Space for shields. The original
demo has four levels and one music track.
