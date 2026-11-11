---
id: putt-putt-travels-through-time-demo
kind: game
title: "Putt-Putt Travels Through Time Demo"
summary: >-
  Choose a time portal and help Putt-Putt recover his history report in the
  original Macintosh adventure demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1997
architectures:
- 68k
default_architecture: 68k
launch_enabled: false
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: c790ba27 native-equivalent runtime
    architecture: 68k
    environment: >-
      Complete unchanged publisher-FTP demo using its original 68040 version,
      with default guest memory, display, sound and preferences. The original
      documented Escape key finishes the current cutscene. Clicking the
      prehistoric time portal moves Putt-Putt into the prehistoric area. The
      omitted-click control stays in the time continuum. Active and omitted-
      input runs have RGB-identical pre-input captures and reach 6791 frontend
      and 7391 guest ticks with zero exhausted frames. Actual captures
      inspected. This verifies bounded native navigation. The preserved PowerPC
      version has a startup memory-write fault and is not enabled. Browser
      controls/pacing, later puzzles, persistence and independent hardware
      accuracy remain unverified. Frozen native binary adb59249 uses runtime
      source 469a0fd8 equivalent to merged c790ba27.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6043
runtime:
  executable_path: Putt Time Demo/TimeDemo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/PuttTimeDemo.sit
    expected_sha256: ca68dd1c4985108ece3a614fc630e1f9959e14a7baa8f9d147550e898a06f44f
    expected_size: 17242188
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/PuttTimeDemo.sit
    - https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
    - https://github.com/benletchford/systemless/issues/6043
    license: Maintainer-accepted promotional demo
    rights_holder: Humongous Entertainment and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed original notices.
      Preserve the complete archive, bundled notices, credits and demo limits.
    notes: >-
      Original distribution preserved from ftp.cavedog.com/humongous. Independent
      unar extraction matches production packing on 10 files, 20 forks and
      10 Finder type, creator and flags records. The complete publisher Read Me
      and 400 selected readable UI, dialog and version resources were reviewed,
      including bundled Apple components. No selected notice resources are compressed.
      All original files and both original executable architectures remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/putt-putt-travels-through-time-demo/gameplay.png
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
      navigation run. Excludes the Mac menu bar, desktop and window framing.
      PNG SHA-256 eff202706637115853c75f6a9b4ade69df6f5807b12cafcc1e77379c6e263195, 201657 bytes.
references:
- https://github.com/benletchford/systemless/issues/6043
---

![Putt-Putt and Pep choose among the time portals](incoming/putt-putt-travels-through-time-demo/gameplay.png)

Join Putt-Putt and Pep on a trip through time. Choose the prehistoric portal, explore the past, and look for Putt-Putt’s missing history report.

The complete original publisher demo and readme are preserved. Native
navigation is verified. Browser play is still being checked.
