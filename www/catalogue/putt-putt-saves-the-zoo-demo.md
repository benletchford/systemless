---
id: putt-putt-saves-the-zoo-demo
kind: game
title: Putt-Putt Saves the Zoo Demo
summary: >-
  Help Putt-Putt and Pep explore the zoo and solve a raft puzzle in the original
  Macintosh adventure demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1995
architectures:
- ppc
default_architecture: ppc
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 469a0fd8 native candidate
    architecture: ppc
    environment: >-
      Complete unchanged publisher-FTP demo at default settings. Choose the original
      Set to 256 Colors option and dismiss the original promotional dialog with OK.
      Select the inventory log and apply it to the raft. The log leaves inventory and
      fills the raft gap. The matched no-click run keeps the log and the incomplete raft.
      Active and omitted-input runs have RGB-identical pre-input captures and reach 9274
      frontend and guest ticks with zero exhausted frames. Actual captures inspected.
      This verifies bounded native interaction. Browser input, pacing, later puzzles,
      persistence and independent hardware accuracy remain unverified. Requires the
      startup and palette fixes in PRs 6035 and 6037.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6038
runtime:
  executable_path: Puttzoo Demo/Puttzoo Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 38638b822e3750ea96b570abf0ee66f12bea43973495fe8ac18c57c90657d391
    size_bytes: 19199232
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/PuttZooDemo.sit.bin
    - https://ftp.zx.net.nz/pub/archive/ftp.cavedog.com/humongous/
    - https://github.com/benletchford/systemless/issues/6038
    license: Maintainer-accepted promotional demo
    rights_holder: Humongous Entertainment and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance,
      without an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in the reviewed original notices. Preserve the
      complete archive, bundled notices, credits and demo limits.
    notes: >-
      Complete original MacBinary-wrapped StuffIt distribution preserved from
      ftp.cavedog.com/humongous, re-downloaded and verified byte-for-byte against the tested
      archive. Independent unar extraction agrees with production packing on nine files,
      eighteen forks and nine Finder records. The complete 3190-byte publisher Read Me
      and 396 readable UI, dialog and version resources were reviewed, including
      bundled QuickTime, Sound and Sound Manager components. No selected notice resources
      are compressed. The Read Me resource fork contains only window geometry and style
      records. All original files remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 0ee2ee8bfd3f8ec873fb2f031498ecaffd9b02c91c72a595b1e4626165e897ce
    size_bytes: 231197
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
      PNG SHA-256 0ee2ee8bfd3f8ec873fb2f031498ecaffd9b02c91c72a595b1e4626165e897ce,
      231197 bytes.
references:
- https://github.com/benletchford/systemless/issues/6038
---

![Putt-Putt beside the raft after placing the inventory log](https://assets.systemless.org/catalogue/media/sha256/0e/0ee2ee8bfd3f8ec873fb2f031498ecaffd9b02c91c72a595b1e4626165e897ce.png)

Explore a small part of the zoo with Putt-Putt and Pep. Point at characters and scenery, collect useful objects, and use inventory items to solve the demo’s puzzles.

The complete original publisher demo and its readme are preserved. Native
interaction has been verified. Browser play is still being checked.
