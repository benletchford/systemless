---
id: hellcats-over-the-pacific
kind: game
title: Hellcats Over the Pacific
summary: >-
  Watch Graphic Simulations Corporation's original automated Macintosh flight
  demo.
developer: Parsoft Interactive
publisher: Graphic Simulations Corporation
year: 1991
architectures:
- 68k
default_architecture: 68k
category: Simulation
compatibility:
  status: boots
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: 0.51.0 + current generic File Manager fixes
    architecture: 68k
    environment: >-
      Deterministic headless run of the MacBinary-preserved 1992 Apple demo-CD files
      at 800 by 600 in 256 colours. The original selector accepted Play Demo and
      rendered aircraft on an airfield at tick 1804. This is an automated demonstration, not
      controllable flight; browser verification remains pending.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2741
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: 670d2018910f459b13a4a78153206e34e73b67388185fc249e59c3c6b093da5c
    size_bytes: 474980
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
    rights_holder: Graphic Simulations Corporation and Parsoft Interactive rights holders
    permission: >-
      Apple distributed these three original Macintosh demo files on its 1992
      Macintosh Demo Games CD. The bundled Graphic Simulations Corporation read-me identifies
      an automated demo and warns that keyboard input exits it. No express
      redistribution licence was found; this preservation basis applies only to the original
      promotional files, not to retail game media.
    notes: "The 474,980-byte ZIP, SHA-256 670d2018910f459b13a4a78153206e34e73b67388185fc249e59c3c6b093da5c, is a transport wrapper assembled from the CD's three MacBinary-preserved files: Hellcats Demo, Pacific Conflict, and Hellcats Read Me. Their MacBinary contents are unchanged; the ZIP itself is not a publisher archive."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 28be5474b78b93f62dc441ee0fc8b1ab773ba10d23b8537962c7f003e6b0a813
    size_bytes: 12158
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2741
    permission: >-
      Fresh deterministic capture made for this catalogue entry. Underlying game
      artwork remains the property of its rights holders.
    notes: >-
      The play harness saved the 800-by-580 game-content framebuffer rectangle at
      (0,20) directly, excluding the Classic Mac menu bar without changing game pixels.
      PNG SHA-256 28be5474b78b93f62dc441ee0fc8b1ab773ba10d23b8537962c7f003e6b0a813,
      12,158 bytes.
references:
- https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
- https://github.com/benletchford/systemless/issues/2741
---

## Watch the automated demo

![Hellcats demo aircraft on an airfield](https://assets.systemless.org/catalogue/media/sha256/28/28be5474b78b93f62dc441ee0fc8b1ab773ba10d23b8537962c7f003e6b0a813.png)

Graphic Simulations Corporation's original Macintosh demonstration opens with
a selector. Choose **Play Demo** to watch the aircraft sequence. The publisher's
read-me says the sequence is fully automated and warns that keyboard input
exits it. This listing does not offer controllable flight.

The preserved files are the 68K promotional application, its Pacific Conflict
data, and the accompanying read-me from Apple's 1992 demo CD. They are not the
complete commercial game. The ZIP is a new transport wrapper around those
unchanged MacBinary files, not an original publisher archive.
