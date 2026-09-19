---
id: seismic-duck
kind: game
title: Seismic Duck
summary: >-
  Learn how geologists use acoustic reflection to explore subsurface strata and
  drill for oil before going broke in Arch D. Robison's 1998 PowerPC seismology game.
developer: Arch D. Robison
publisher: Arch D. Robison
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Strategy
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: 2026-09-18
    tester: Catalogue maintainer
    systemless_version: 0.41.13
    architecture: ppc
    environment: Deterministic headless run from the Info-Mac mirror distribution archive
    status: playable
    evidence: https://github.com/benletchford/systemless.org/issues/281
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 178d439c4a791c78eaac0d85b09d36142353095798de902db799f1700693e842
    size_bytes: 129713
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://mirrors.nic.funet.fi/pub/mac/info-mac/game/seismic-duck-16.hqx
    license: Freeware
    rights_holder: Arch D. Robison
    permission: "The bundled documentation and Info-Mac submission state: \"Distribution: Seismic Duck 1.6 is freeware. See duckumentation for terms. The author requests comments on its game and educational value.\""
    notes: >-
      Untouched Info-Mac BinHex distribution archive retrieved from
      mirrors.nic.funet.fi. The 129,713-byte file has SHA-256
      178d439c4a791c78eaac0d85b09d36142353095798de902db799f1700693e842. Developed by Arch D. Robison. The original author retains
      copyright and has no active modern commercial releases or storefront listings
      for this title.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 88b08b2662cb3f29473041b636045317b0732d86e37a24beac272ab4fabc5d00
    size_bytes: 4771
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless.org/issues/281
    permission: >-
      Original screenshot captured for this catalogue at the maintainer's request.
      Underlying game artwork remains the property of its rights holder.
    notes: >-
      Fresh deterministic capture from the unchanged distribution archive on
      2026-09-18, cropped exactly to the game's 640-by-460 content surface. It excludes the
      Classic Mac menu bar, browser, website, host desktop and emulator chrome.
references:
- https://mirrors.nic.funet.fi/pub/mac/info-mac/game/seismic-duck-16.hqx
---

## Subsurface geophysics and exploration strategy

![Seismic Duck gameplay](https://assets.systemless.org/catalogue/media/sha256/88/88b08b2662cb3f29473041b636045317b0732d86e37a24beac272ab4fabc5d00.png)

Released in 1998 by computer scientist Arch D. Robison, *Seismic Duck* is an educational geophysics simulation and strategy game that models real-world reflection seismology. Players explore geological subterranean formations by firing acoustic sound pulses from an air gun and observing the simulated wave reflections echoing off rock layers, faults, and hydrocarbon traps.

The objective is to analyze the seismic waveform returns and synthetic seismograms to locate oil and gas deposits trapped beneath structural anticlines, then strategically place drilling rigs to extract petroleum before drilling expenses exhaust the operating budget. With multiple difficulty settings ranging from elementary-level demonstrations to complex geophysical surveys, the game balances accessible mechanics with authentic wave propagation physics.

## Native PowerPC execution

Systemless runs *Seismic Duck* natively in its PowerPC runtime environment. The simulation executes CFM code utilizing MathLib floating-point routines—including exponential decay and trigonometric calculations for acoustic attenuation and wave front spreading—alongside QuickDraw color rendering to generate real-time 2D wave propagation fields in the browser.
