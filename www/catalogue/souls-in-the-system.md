---
id: souls-in-the-system
kind: game
title: Souls in the System
summary: >-
  Pilot a fighter through three fast-paced levels in Terminal Sunset's original
  Macintosh demo.
developer: Terminal Sunset Software
publisher: StarPlay Productions
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Arcade
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-27"
    tester: Catalogue maintainer
    systemless_version: master at dd24044c6af1
    architecture: 68k
    environment: >-
      Optimized Chrome browser run from the promoted demo archive through the
      startup prompt and player setup into live first-level gameplay; display
      and audio remained stable at about 60 host frames per second.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2975
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 808a054cbcc8d02bf64f33128e5e852e445f339cda47cdd3983ba5505dcacad7
    size_bytes: 5338547
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/souls-in-the-system
    - https://static.classicmacdemos.com/demos/souls-in-the-system/README.txt
    rights_holder: Terminal Sunset Software and StarPlay Productions
    permission: >-
      The bundled README identifies this as a promotional demo but does not state a
      broader redistribution licence. This unchanged demo was distributed for
      promotional play, with ordering information for the full game.
    notes: >-
      Original 5,338,547-byte StuffIt archive, SHA-256
      808a054cbcc8d02bf64f33128e5e852e445f339cda47cdd3983ba5505dcacad7. It contains the 68K application, three-level
      demo data, recorded films, bundled sound assets, and the publisher's readme.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: df78466523cead9217a0bf47a99f9715835efeae142c39cc58433002b273a004
    size_bytes: 437552
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://classicmacdemos.com/souls-in-the-system
    - https://github.com/benletchford/systemless/issues/2975
    permission: >-
      Gameplay captured from the unchanged promotional demo. Underlying game artwork
      remains the property of Terminal Sunset Software and StarPlay Productions.
    notes: >-
      Deterministic Systemless capture from the three-level demo at 640x480. PNG
      SHA-256 df78466523cead9217a0bf47a99f9715835efeae142c39cc58433002b273a004, 437,552
      bytes.
references:
- https://classicmacdemos.com/souls-in-the-system
- https://github.com/benletchford/systemless/issues/2975
---

## Three levels, one fighter

Souls in the System follows ShadowWraith with a fast top-view fighter game.
The original demo offers three levels, recorded films, and customizable
controls. It is an unchanged promotional preview, not the retail game.

At first launch, accept the screen-size prompt. Choose **Start Game**, then
**Use Standard Levels** and **Okay** to enter the demo. The original README
recommends a 640-by-480 display with 256 colors and at least 5.5 MB of free
application memory.

![Souls in the System gameplay](https://assets.systemless.org/catalogue/media/sha256/df/df78466523cead9217a0bf47a99f9715835efeae142c39cc58433002b273a004.png)
