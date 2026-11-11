---
id: freddi-fish-2-demo
kind: game
title: "Freddi Fish 2: The Case of the Haunted Schoolhouse Demo"
summary: >-
  Explore the underwater world with Freddi and Luther while helping their friend
  Tucker in this original Macintosh demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1996
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
      Escape key finishes the current cutscene. Clicking the grey pipe opens a close-up of
      Tucker. The omitted-click control stays in the underwater room. Active and
      omitted-input runs have RGB-identical pre- input captures and reach 6799 frontend and
      7399 guest ticks with zero exhausted frames. Actual captures inspected. This
      verifies bounded native navigation. The preserved PowerPC version has a startup
      memory-write fault and is not enabled. Browser controls/pacing, later puzzles,
      persistence and independent hardware accuracy remain unverified. Frozen native binary
      adb59249 uses runtime source 469a0fd8 equivalent to merged c790ba27.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6043
runtime:
  executable_path: Freddi Fish 2 Demo/Freddi Fish 2 Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: a9b014df33f890adf527b2fde6099468790dd38534413d80c10841c3d044f9fb
    size_bytes: 25456640
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/FreddiFish2Demo.sit.bin
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
      unar extraction matches production packing on 9 files, 18 forks and 9 Finder type,
      creator and flags records. The complete publisher Read Me and 396 selected
      readable UI, dialog and version resources were reviewed, including bundled Apple
      components. No selected notice resources are compressed. All original files and both
      original executable architectures remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 7f08f0b4e774b5e8161df9b86bcdb0d26e1b5e219332f676f7b34923dfe07a6f
    size_bytes: 258895
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
      7f08f0b4e774b5e8161df9b86bcdb0d26e1b5e219332f676f7b34923dfe07a6f, 258895 bytes.
references:
- https://github.com/benletchford/systemless/issues/6043
---

![Freddi and Luther inspect a pipe in their underwater world](https://assets.systemless.org/catalogue/media/sha256/7f/7f08f0b4e774b5e8161df9b86bcdb0d26e1b5e219332f676f7b34923dfe07a6f.png)

Point and click to explore with Freddi and Luther in this small adventure from their haunted-schoolhouse mystery. The inventory bubbles hold useful items for the demo’s puzzles.

The complete original publisher demo and readme are preserved. Native
navigation is verified. Browser play is still being checked.
