---
id: star-wars-jedi-knight-ii-jedi-outcast
kind: game
title: "Star Wars Jedi Knight II: Jedi Outcast Demo"
summary: >-
  The original Power Macintosh demo of Kyle Katarn's lightsaber adventure.
developer: Raven Software
publisher: Aspyr Media
year: 2002
architectures:
- ppc
default_architecture: ppc
category: FPS
launch_enabled: false
compatibility:
  status: broken
  verified:
  - date: "2026-09-28"
    tester: Catalogue maintainer
    systemless_version: development build reported in issue 3319
    architecture: ppc
    environment: >-
      The unchanged demo reaches CFBundleLoadExecutable during its first
      native replay tick. Systemless cannot load the PowerPC Mach-O HID bundle,
      so gameplay and browser launch have not been verified.
    status: broken
    evidence: https://github.com/benletchford/systemless/issues/3319
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/Jedi%20Knight%20II%20Demo.sit
    expected_sha256: 9a77ac3521ba6824a94de4ae40cc42c47f5c1f939f5a73c8e510c38f589026e4
    expected_size: 65747186
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/star-wars-jedi-knight-ii-jedi-outcast
    - https://download.classicmacdemos.com/Jedi%20Knight%20II%20Demo.sit
    license: LucasArts Jedi Knight II demo license
    rights_holder: LucasArts Entertainment Company LLC and its licensors
    permission: >-
      The bundled license.txt permits free electronic distribution of copies of
      the complete demo in its original form. The archive is unchanged and
      includes the license and all original demo files.
    notes: >-
      The original 65,747,186-byte StuffIt archive has SHA-256
      9a77ac3521ba6824a94de4ae40cc42c47f5c1f939f5a73c8e510c38f589026e4.
      Its Classic game executable and JKGameLib are PowerPC PEF; the game has
      a pwpc cfrg and no 68K CODE resources. The bundled Read Me First identifies
      demo version 1.03 and requires a PowerPC G3 or G4 Macintosh.
references:
- https://classicmacdemos.com/star-wars-jedi-knight-ii-jedi-outcast
- https://github.com/benletchford/systemless/issues/3319
---

## The original Mac demo

This is the unchanged 2002 Mac demo package. Its Classic Mac executable is
PowerPC only. Browser launch is disabled while Systemless work on the demo's
PowerPC library loading continues.
