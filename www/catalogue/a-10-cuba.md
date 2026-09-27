---
id: a-10-cuba
kind: game
title: A-10 Cuba! Demo
summary: Fly the Warthog in Parsoft's original playable Macintosh flight-sim demo.
developer: Parsoft Interactive
publisher: Parsoft Interactive
year: 1996
architectures:
- 68k
default_architecture: 68k
category: Simulation
compatibility:
  status: playable
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: c1e4685bc77730ad04977c2c2c5d6987814b1c
    architecture: 68k
    environment: >-
      Deterministic headless replay of the unchanged demo. Selected Heavy Metal,
      entered Mission I, reached live 3D flight, then sent the bundled Read Me's throttle,
      landing-gear, and chase-view commands. Browser launch has not yet been verified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/3037
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: a78c0d6b7719d69a20dc9869f9b49d403eeae7d97765c80cce8d37e14dbbd852
    size_bytes: 2765031
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/a-10-cuba
    - https://download.classicmacdemos.com/A-10%20Cuba%20Demo.sit
    - https://static.classicmacdemos.com/demos/a-10-cuba/README.txt
    rights_holder: Parsoft Interactive
    permission: >-
      This is the unchanged purpose-built promotional Macintosh demo. Its included
      Read Me explicitly identifies it as the A-10 Cuba! Demo, explains how to play, and
      advertises the full retail game. No retail mission or modified application is
      included. The archive has no express redistribution clause.
    notes: >-
      Original 2,765,031-byte StuffIt archive, SHA-256
      a78c0d6b7719d69a20dc9869f9b49d403eeae7d97765c80cce8d37e14dbbd852. It contains a runnable 68K demo application,
      demo data, and a PPC support library; the 68K application was used for
      verification.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 70e6d316042459430a6dd1053a609c319e51c41203f784151c4ea48f6eaed996
    size_bytes: 12274
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/3037
    permission: >-
      Fresh gameplay capture from the unchanged promotional demo for this catalogue
      entry. Underlying game artwork remains its owner's property.
    notes: >-
      Exact 640-by-480 flight-content crop at (80,60) of a deterministic 800-by-600
      Systemless framebuffer after switching to chase view. The crop excludes the
      desktop and menu bar without changing game pixels. PNG SHA-256
      70e6d316042459430a6dd1053a609c319e51c41203f784151c4ea48f6eaed996, 12,274 bytes.
references:
- https://classicmacdemos.com/a-10-cuba
- https://static.classicmacdemos.com/demos/a-10-cuba/README.txt
---

## Into the air

![A-10 Cuba demo Warthog in chase view above a green landscape](https://assets.systemless.org/catalogue/media/sha256/70/70e6d316042459430a6dd1053a609c319e51c41203f784151c4ea48f6eaed996.png)

The A-10 Cuba! demonstration lets you select a mission and fly the Warthog
across its 3D island terrain. The included Read Me explains the keyboard
controls, from throttle and landing gear to different camera views. This
capture shows the chase view after the plane responded to those inputs.

This is Parsoft's original Macintosh promotional demo, not the retail game.
Systemless reached interactive flight in a deterministic replay. Browser
launch remains disabled until the route is manually tested and approved.
