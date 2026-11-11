---
id: freddi-fish-demo
kind: game
title: Freddi Fish and the Case of the Missing Kelp Seeds Demo
summary: >-
  Join Freddi and Luther on an underwater search in Humongous Entertainment’s
  original Macintosh adventure demo.
developer: Humongous Entertainment
publisher: Humongous Entertainment
year: 1995
architectures:
- ppc
default_architecture: ppc
launch_enabled: true
category: Puzzle
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 469a0fd8 native candidate
    architecture: ppc
    environment: >-
      Complete unchanged publisher-FTP demo at default settings. Choose the original
      Set to 256 Colors option and dismiss the original promotional dialog with OK.
      Clicking the fish in the left glass window triggers its character animation. The
      matched no-click run leaves that character unchanged. Active and omitted-input runs
      have RGB-identical pre-input captures and reach 9145 frontend and guest ticks
      with zero exhausted frames. Actual captures inspected. This verifies bounded native
      interaction. Browser input, pacing, later puzzles, persistence and independent
      hardware accuracy remain unverified. Requires the startup and palette fixes in PRs
      6035 and 6037.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6038
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: 4dd4f9e1 private browser equivalent
    architecture: ppc
    status: playable
    environment: >-
      Chromium worker and WebGL at the default PowerPC clock, display, memory
      and sound settings. The complete unchanged archive and both main/worker
      Wasm response bodies are pinned by size and SHA-256. The private preview
      uses original archive bytes at loopback-only URLs. Independent fetches
      from the real production origin verify all promoted archives and images
      through browser CORS. Choose the original Set to 256 Colors button,
      dismiss the publisher screen with OK, and allow the original CD-ROM test,
      introduction and room entrance to finish. Clicking the fish in the left
      glass window enlarges it into its talking animation. The omitted-click
      control keeps the small figure. Both active and idle runs pass separate
      20-second interaction and sustained-scene windows. Maximum worker frames
      are 27.4, 43, 27, 28.1 ms and minimum audio queues are 466, 329.9, 472,
      304.5 ms. Actual input and control captures were inspected. Pre-input
      world state matches, with minor ambient animation differences. The opening
      display prompt has a black background and missing explanatory text; its
      original labeled buttons remain readable and usable. Later puzzles,
      persistence, audio fidelity, independent hardware accuracy and released
      public play remain unverified. Private preview source 0030d99f uses
      runtime source 469a0fd8, whose runtime tree matches merged 4dd4f9e1.
    evidence: https://github.com/benletchford/systemless/issues/6040
runtime:
  executable_path: Freddi Fish Demo/Freddi Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: sha256
    sha256: 8a170bca098e33cc26b0d045e5f52779e392ccec770c585801946727607e2c2a
    size_bytes: 8249984
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://discmaster.textfiles.com/file/11876/ftp.cavedog.com.tar/ftp.cavedog.com/humongous/FreddiFishDemo.sit.bin
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
    sha256: 2a4e42897cc1b57d2ec04a69467cb3c929484fb032dfc9c50457821fd7bf823d
    size_bytes: 238921
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
      PNG SHA-256 2a4e42897cc1b57d2ec04a69467cb3c929484fb032dfc9c50457821fd7bf823d,
      238921 bytes.
references:
- https://github.com/benletchford/systemless/issues/6038
- https://github.com/benletchford/systemless/issues/6040
---

![Freddi and Luther meet a fish inside an underwater wreck](https://assets.systemless.org/catalogue/media/sha256/2a/2a4e42897cc1b57d2ec04a69467cb3c929484fb032dfc9c50457821fd7bf823d.png)

Point and click to explore an underwater mystery with Freddi Fish and Luther. This original Macintosh demo offers a small adventure from their search for the missing kelp seeds.

The complete original publisher demo and its readme are preserved. Choose
**Set to 256 Colors** in the opening dialog, then **OK** on the publisher
screen. Allow the CD-ROM test and introduction to finish. The opening dialog
can have missing explanatory text, while its buttons remain readable.
