---
id: falcon-mc
kind: game
title: Falcon MC
summary: Fly an F-16 against MiG-29s in Spectrum HoloByte's Macintosh Color demo.
developer: Spectrum HoloByte
publisher: Spectrum HoloByte
year: 1992
architectures:
- 68k
default_architecture: 68k
category: Simulation
compatibility:
  status: boots
  verified:
  - date: "2026-09-23"
    tester: Catalogue maintainer
    systemless_version: "0.53.0"
    architecture: 68k
    environment: >-
      Deterministic headless run of the unchanged Macintosh demo at 800-by-600.
      Accepted the 16-colour mode prompt, opened the demo menu, and entered Instant Action
      to reach the live F-16 cockpit. The colour-switch prompt renders blank in
      Systemless and browser launch has not been approved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2492
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: d8b3d2f90c4fc0ed861815bb138c450b60dbca1f02d446c0d438eca292832677
    size_bytes: 448500
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://macintoshgarden.org/games/falcon-mc
    - https://www.macintoshrepository.org/3168-falcon-mc
    - https://groups.google.com/g/comp.sys.mac.games/c/Q3xwbznh0wo
    rights_holder: Falcon rights holders
    permission: >-
      This unchanged, intentionally limited demo was distributed to promote Falcon
      MC. Its included Description identifies a demo, and the game's own menu invites a
      sample Instant Action mission. A contemporary 1992 player report independently
      discusses this demo. No copy-protected retail disk or manual is included; the
      archive has no express redistribution clause.
    notes: >-
      Original 448,500-byte StuffIt demo archive. SHA-256
      d8b3d2f90c4fc0ed861815bb138c450b60dbca1f02d446c0d438eca292832677. MD5 7ab8d7e9124b71a3c2d5a9c8ff6263e0
      matches Macintosh Garden and SHA-1 22554883dfe54ce98d9c0f4f5efe7ae97d966613 matches
      Macintosh Repository. This is Falcon MC, the 1992 colour successor to Falcon.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 67796c7683b4051c8efbf93db2f3f21e6dba575c5e961e9870ba6101e764f38c
    size_bytes: 90266
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2492
    permission: >-
      Fresh deterministic cockpit capture made for this catalogue entry. Underlying
      game artwork remains the property of its rights holders.
    notes: >-
      Captured the 512-by-322 game-content region at (144,128) after selecting
      Instant Action. The crop excludes host UI and the Mac menu bar without changing game
      pixels. PNG SHA-256
      67796c7683b4051c8efbf93db2f3f21e6dba575c5e961e9870ba6101e764f38c, 90,266 bytes.
references:
- https://macintoshgarden.org/games/falcon-mc
- https://www.macintoshrepository.org/3168-falcon-mc
- https://groups.google.com/g/comp.sys.mac.games/c/Q3xwbznh0wo
---

## F-16 Instant Action

![Falcon MC demo F-16 cockpit in Instant Action](https://assets.systemless.org/catalogue/media/sha256/67/67796c7683b4051c8efbf93db2f3f21e6dba575c5e961e9870ba6101e764f38c.png)

Take the controls of an F-16 and face MiG-29s in a sample combat mission.
Falcon MC adds colour scenery and a detailed cockpit to the Macintosh flight
simulator series. This demo offers Instant Action rather than the retail game's
full campaign.

## The original demo

This is Spectrum HoloByte's promotional Falcon MC demo, not the copy-protected
retail game. Systemless reaches the cockpit after the demo's 16-colour prompt,
but that prompt does not yet draw its text or buttons correctly. Browser launch
therefore remains disabled while the [alert-rendering issue](https://github.com/benletchford/systemless/issues/2491)
is tracked.
