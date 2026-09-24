---
id: simant
kind: game
title: SimAnt
summary: Explore Maxis's original Macintosh colour demonstration of its ant-colony simulation.
developer: Maxis
publisher: Maxis
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Simulation
compatibility:
  status: boots
  verified:
  - date: "2026-09-25"
    tester: Catalogue maintainer
    systemless_version: "0.59.0"
    architecture: 68k
    environment: >-
      Deterministic 800-by-600, 256-colour run of the MacBinary-preserved
      application from Apple's 1992 Macintosh Demo Games CD. The 16-colour
      recommendation dialog was dismissed and an animated colony scene rendered.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2770
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: incoming
    path: catalogue/incoming/simant/simant-color-demo-1992-apple-cd.zip
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
    rights_holder: Electronic Arts Inc. and successors to Maxis
    permission: >-
      Maxis supplied this standalone promotional demonstration for Apple's
      Macintosh Demo Games CD. This listing distributes only that original
      demonstration application, not the commercial SimAnt game. No separate
      licence granting broader reuse was found in its one-file CD folder.
    notes: >-
      The 391,705-byte ZIP (SHA-256
      9bce3e2686a50a0510e9b948aed8947f632b477fd78e159143ceeb9fa919e94f)
      is a new transport wrapper containing one unchanged MacBinary extraction
      of the 68K APPL/SAND application. Its 590,208-byte MacBinary file has SHA-256
      34cff62dafb68ce48f35202842e6443e7d5e8f6285c90a803b08e48189c8fbc3.
      The source CD ZIP has SHA-1 0e3362ed5e0a68e0b97ad51dfc7804db012a4936.
- id: colony-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/simant/simant-colony.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2770
    permission: >-
      Fresh Systemless capture of the unchanged promotional demonstration.
      The underlying artwork remains the property of its rights holders.
    notes: >-
      Direct 736-by-544 colony-viewport capture, without desktop or window
      chrome and without modifying the demo's pixels. The 1,201,854-byte PNG
      has SHA-256 b8587c6bf6f519121ee0d490e31ed03f1935225201bd897df07c6ca794c47ab1.
references:
- https://archive.org/details/apple-the-macintosh-demo-games-cd-1992-10-english-cd
- https://github.com/benletchford/systemless/issues/2770
---

## A promotional look at the colony

![Ants moving through the SimAnt Color Demo colony](incoming/simant/simant-colony.png)

This is the original *SimAnt Color Demo* distributed on Apple's 1992
Macintosh Demo Games CD. It opens an animated ant-colony scene after a display
mode prompt. The available evidence does not establish that its controls offer
the complete game's interactive modes, so this listing presents it as a
promotional demonstration rather than a playable copy of *SimAnt*.

The archive preserves the demo application and its resource fork in MacBinary
form. It contains no retail game disk or installed commercial files. Browser
launch remains disabled pending a release-mode browser check.
