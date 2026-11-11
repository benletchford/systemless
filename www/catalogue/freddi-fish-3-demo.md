---
id: freddi-fish-3-demo
kind: game
title: "Freddi Fish 3: The Case of the Stolen Conch Shell Demo"
summary: >-
  Visit a tropical island with Freddi and Luther and find a way through the net
  in this original Macintosh demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1998
architectures:
- 68k
default_architecture: 68k
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: c790ba27 native-equivalent runtime
    architecture: 68k
    environment: >-
      Complete unchanged publisher-FTP demo using its original 68040 version, with
      default guest memory, display, sound and preferences. The original documented
      Escape key finishes the current cutscene. Click the visible red direction arrow above
      the buoy. The active run enters the underwater net area. The omitted-click
      control stays at Pelican Sam’s island. Active and omitted-input runs have RGB-identical
      pre-input captures and reach 3191 frontend and 3791 guest ticks with zero
      exhausted frames. Actual captures inspected. This verifies bounded native navigation.
      The preserved PowerPC version has a startup memory-write fault and is not enabled.
      Browser controls/pacing, later puzzles, persistence and independent hardware
      accuracy remain unverified. Frozen native binary adb59249 uses runtime source
      469a0fd8 equivalent to merged c790ba27.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6043
runtime:
  executable_path: Freddi Fish 3 Demo/F3-Mdemo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 4cdccfc67aa34b6b14f1202bb90c4b7c0fc15d6d918fde951373a1ff652f7b71
    size_bytes: 8348692
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/FreddiFish3Demo.sit
    - https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
    - https://github.com/benletchford/systemless/issues/6043
    license: Maintainer-accepted promotional demo
    rights_holder: Humongous Entertainment and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed original notices. Preserve
      the complete archive, bundled notices, credits and demo limits.
    notes: >-
      Original distribution preserved from ftp.cavedog.com/humongous. Independent
      unar extraction matches production packing on 10 files, 20 forks and 10 Finder type,
      creator and flags records. The complete publisher Read Me and 400 selected
      readable UI, dialog and version resources were reviewed, including bundled Apple
      components. No selected notice resources are compressed. All original files and both
      original executable architectures remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: f68d604161716db0cd196832505e66d6c57438de35171459db45121b16dffb4e
    size_bytes: 290498
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6043
    rights_holder: Humongous Entertainment and original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted
      promotional demo. Underlying artwork retains its original copyright.
    notes: >-
      Unscaled 640x480 original game surface, cropped from the verified native
      navigation run. Excludes the Mac menu bar, desktop and window framing. PNG SHA-256
      f68d604161716db0cd196832505e66d6c57438de35171459db45121b16dffb4e, 290498 bytes.
references:
- https://github.com/benletchford/systemless/issues/6043
---

![Freddi and Luther reach the underwater net area](https://assets.systemless.org/catalogue/media/sha256/f6/f68d604161716db0cd196832505e66d6c57438de35171459db45121b16dffb4e.png)

Explore the island with Freddi and Luther during their stolen-conch-shell mystery. Follow the red direction arrow from Pelican Sam’s island to the underwater net area, then look for a way through.

The complete original publisher demo and readme are preserved. Native
navigation is verified. Browser play is still being checked.
