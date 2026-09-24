---
id: fa-18-hornet-20
kind: game
title: F/A-18 Hornet 2.0
summary: Fly Graphic Simulations Corporation's original Macintosh 68K demo.
developer: Graphic Simulations Corporation
publisher: Graphic Simulations Corporation
year: 1995
architectures:
- 68k
default_architecture: 68k
category: Simulation
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: 0.51.0 + File Manager FCB fix
    architecture: 68k
    environment: "Deterministic headless run of the unchanged original demo at 800-by-600 in 256 colours with a 20 MB application partition. Selected the demo flight and reached the fully rendered cockpit and runway. The generic File Manager fix landed in PR #2735. The promoted archive was also tested in the browser: interactive demo, Fly Demo, and fully rendered cockpit and runway."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2713
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 86ccdaf2b83c84804b2bdbbac0db7d445bff106d67f8f5515f9a38d21bbb2f85
    size_bytes: 2537895
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/fa-18-hornet-20
    rights_holder: Graphic Simulations Corporation
    permission: >-
      The publisher's bundled F/A-18 Hornet 2.0 Demo Docs explicitly says that the
      demo may be freely distributed. This is the unchanged promotional 68K archive, not
      the retail game, a registration bypass, or the later PowerPC-only Hornet 3 demo.
    notes: >-
      Original 2,537,895-byte StuffIt 5 archive, SHA-256
      86ccdaf2b83c84804b2bdbbac0db7d445bff106d67f8f5515f9a38d21bbb2f85. Contains the 68K application, demo data,
      six demonstration scripts, and the publisher's self-contained documentation.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c164b7dd8619be0690a34d83059bcae79efadc6997b1ddbeb10e36044b636a23
    size_bytes: 48813
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2713
    permission: >-
      Fresh deterministic gameplay capture made for this catalogue entry. Underlying
      game artwork remains the property of its rights holder.
    notes: >-
      Pixel-preserving crop of the 640-by-480 game-content region at (80,60) after
      entering the demo flight. The crop excludes emulator framing and desktop. PNG
      SHA-256 c164b7dd8619be0690a34d83059bcae79efadc6997b1ddbeb10e36044b636a23, 48,813 bytes.
references:
- https://classicmacdemos.com/fa-18-hornet-20
- https://github.com/benletchford/systemless/issues/2713
---

## In the cockpit

![F/A-18 Hornet 2.0 demo cockpit and runway](https://assets.systemless.org/catalogue/media/sha256/c1/c164b7dd8619be0690a34d83059bcae79efadc6997b1ddbeb10e36044b636a23.png)

Select the demo flight from the interactive briefing to enter the Hornet's
cockpit. The forward view shows the runway through the head-up display, with
radar, instruments, and flight controls surrounding it.

## The original Macintosh demo

This is Graphic Simulations Corporation's 1995 68K promotional version. Its
archive includes the demo application, flight data, six demonstration scripts,
and the original documentation. The publisher's bundled Demo Docs permits
free distribution of the demo. The complete commercial game and the later
PowerPC-only Hornet 3 are not included.
