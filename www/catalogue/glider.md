---
id: glider
kind: game
title: Glider 4.06 Demo
summary: Steer a paper glider through a miniature house in the original Macintosh demo.
developer: John Calhoun
publisher: Casady & Greene
year: 1991
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: 578c699b76948a39ffadae76640eb466375dc203
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged Glider 4.06 demo. Chose the
      two-colour startup mode, selected mouse controls from the game's Options menu,
      started a new game, and steered the glider right into the next room. BasiliskII reached
      the starting room with the same script but did not reproduce that room advance;
      native parity and browser launch are not yet verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3054
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: a3b0678a93a095be8b4c49e0f9c8d37fddb3fc198d5e0adb4c4868850fa12e2c
    size_bytes: 357942
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/glider
    - https://download.classicmacdemos.com/Glider%20Demo.sit
    - https://static.classicmacdemos.com/demos/glider/README.txt
    - https://github.com/EngineersNeedArt/SoftDorothy-CasadyGreeneProjects
    rights_holder: Glider rights holders
    permission: >-
      This unchanged Casady & Greene promotional demo is not the retail game. Its
      included 1992 Read Me identifies Glider 4.06 Demo and directs players to the
      publisher for more information. Classic Macintosh Game Demos records it on 30
      contemporary cover discs. No express redistribution clause is present.
    notes: >-
      Original 357,942-byte StuffIt archive, SHA-256
      a3b0678a93a095be8b4c49e0f9c8d37fddb3fc198d5e0adb4c4868850fa12e2c. The tested Glider Demo 4.06 application has a
      runnable 68K CODE resource.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0b04aa438f0ab74e40b34c4dff85a8cfacbe98b3320fd837bef4187d9d0dc20f
    size_bytes: 23080
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3054
    permission: >-
      Fresh gameplay capture from the unchanged promotional demo for this catalogue
      entry. Underlying game artwork remains its owners' property.
    notes: >-
      Exact 512-by-343 game-content crop at (144,129) of a deterministic 800-by-600
      Systemless framebuffer after steering into another room. The crop excludes the Mac
      menu bar and desktop without changing game pixels. PNG SHA-256
      0b04aa438f0ab74e40b34c4dff85a8cfacbe98b3320fd837bef4187d9d0dc20f, 23,080 bytes.
references:
- https://classicmacdemos.com/glider
- https://static.classicmacdemos.com/demos/glider/README.txt
- https://github.com/EngineersNeedArt/SoftDorothy-CasadyGreeneProjects
---

## A little paper plane

![Glider demo paper plane flying past a bookshelf](https://assets.systemless.org/catalogue/media/sha256/0b/0b04aa438f0ab74e40b34c4dff85a8cfacbe98b3320fd837bef4187d9d0dc20f.png)

Glider turns ordinary household rooms into a side-scrolling obstacle course
for a small paper plane. Vents, furniture, and the floor shape its route.
This capture shows the plane by a bookshelf after mouse steering moved it
right from the starting room; the room name, score, and remaining lives are
visible at the top of the game surface.

This is the original Casady & Greene demonstration, not the retail release.
Systemless reached interactive Demo House play with the unchanged archive.
A separate BasiliskII run reached the starting room but did not reproduce
the scripted mouse-steered room change. Browser launch remains disabled until
manual testing and approval.
