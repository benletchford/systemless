---
id: freddi-fish-demo
kind: game
title: Freddi Fish and the Case of the Missing Kelp Seeds Demo
summary: >-
  Join Freddi and Luther on an underwater search in Humongous Entertainment’s original Macintosh adventure demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1995
architectures:
- ppc
default_architecture: ppc
launch_enabled: false
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 469a0fd8 native candidate
    architecture: ppc
    status: playable
    environment: >-
      Complete unchanged publisher-FTP demo at default settings. Choose the original
      Set to 256 Colors option and dismiss the original promotional dialog with OK.
      Clicking the fish in the left glass window triggers its character animation. The matched no-click run leaves that character unchanged.
      Active and omitted-input runs have RGB-identical pre-input captures and reach
      9145 frontend and guest ticks with zero exhausted frames. Actual captures
      inspected. This verifies bounded native interaction. Browser input, pacing,
      later puzzles, persistence and independent hardware accuracy remain unverified.
      Requires the startup and palette fixes in PRs 6035 and 6037.
    evidence: https://github.com/benletchford/systemless/issues/6038
runtime:
  executable_path: Freddi Fish Demo/Freddi Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: url
    url: https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/FreddiFishDemo.sit.bin
    expected_sha256: 8a170bca098e33cc26b0d045e5f52779e392ccec770c585801946727607e2c2a
    expected_size: 8249984
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/FreddiFishDemo.sit.bin
    - https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
    - https://github.com/benletchford/systemless/issues/6038
    license: Maintainer-accepted promotional demo
    rights_holder: Humongous Entertainment and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed original notices.
      Preserve the complete archive, bundled notices, credits and demo limits.
    notes: >-
      Complete original MacBinary-wrapped StuffIt distribution preserved from
      ftp.cavedog.com/humongous, re-downloaded and verified byte-for-byte against the
      tested archive. Independent unar extraction agrees with production packing on
      nine files, eighteen forks and nine Finder records. The complete 3190-byte
      publisher Read Me and 396 readable UI, dialog and version resources were
      reviewed, including bundled QuickTime, Sound and Sound Manager components.
      No selected notice resources are compressed. The Read Me resource fork contains
      only window geometry and style records. All original files remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/freddi-fish-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6038
    rights_holder: Humongous Entertainment and original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted
      promotional demo. Underlying artwork retains its original copyright.
    notes: >-
      Unscaled 640x480 game surface cropped from the original 800x600 native capture
      after the verified interaction. Excludes the surrounding desktop and title bar.
      PNG SHA-256 2a4e42897cc1b57d2ec04a69467cb3c929484fb032dfc9c50457821fd7bf823d, 238921 bytes.
references:
- https://github.com/benletchford/systemless/issues/6038
---

![Freddi and Luther meet a fish inside an underwater wreck](incoming/freddi-fish-demo/gameplay.png)

Point and click to explore an underwater mystery with Freddi Fish and Luther. This original Macintosh demo offers a small adventure from their search for the missing kelp seeds.

The complete original publisher demo and its readme are preserved. Native
interaction has been verified. Browser play is still being checked.
