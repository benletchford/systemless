---
id: quake-demo
kind: game
title: Quake Demo
summary: >-
  Explore Gothic corridors in the original Macintosh preview of id Software’s
  first-person shooter.
developer: id Software / Lion Entertainment
publisher: MacSoft
year: 1997
architectures:
- ppc
default_architecture: ppc
category: FPS
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 9d234abd runtime code
    architecture: ppc
    environment: >-
      Complete unchanged original Macintosh 1.08.2 preview, PowerPC executable,
      default CPU, partition, sound and initial display. Ordinary Escape, Return, Return
      enters a rendered map despite the existing startup console file-open message. A
      30-tick right-arrow hold rotates the world view against a matched omitted-input
      control; pre-input images match byte for byte. Both bounded scenarios finish at 768
      frontend/guest ticks, zero automatic assertions and zero exhausted frames. Actual
      captures inspected. No independent Mac hardware fidelity or browser qualification
      is claimed. Translation, combat, level completion, persistence and audio fidelity
      remain unqualified. Frozen native binary SHA-256
      f43e3df889a5042e9b23ef0715ba81dd4cc6c5fa1951cdd25a46515fbe6ae84d; its runtime code matches master 9d234abd, with
      catalogue/docs differences only.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5924
runtime:
  executable_path: Quake Demo/Quake Demo
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 742f2b2b51e81281e04cc17967480ba85c8cd65f8cfa2570382f899be7466535
    size_bytes: 6853937
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/download/quake/
    - >-
      https://archive.org/download/macaddict-cd-016/MacAddict%20CD-ROM%20December%201997%2000.jpg
    - https://github.com/benletchford/systemless/issues/5924
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: id Software / MacSoft / Lion Entertainment
    permission: >-
      Accepted under the project promotional-demo policy with explicit maintainer
      authorization on 2026-11-10. Period cover-disc distribution is corroborated, the
      complete original package is preserved, and no explicit redistribution restriction
      was found in the reviewed applicable notices. This records project-policy
      acceptance, not an affirmative publisher licence grant. Original copyright and music
      notices remain intact.
    notes: "Original 6853937-byte SIT, SHA-256 742f2b2b51e81281e04cc17967480ba85c8cd65f8cfa2570382f899be7466535. All 7 files, 14 forks and 7 Finder records match independent unar extraction; strict pack EOF 15087732, SHA-256 4dbf836346c1b1f1ad8839de96ade71051f62b6360727a50f76358cfae0d0d1e. Full Read Me (7784 bytes), Online Manual (15095 bytes) and Troubleshooting (2633 bytes) reviewed, plus 63 selected text/UI/version resources and retained notice matches. MacAddict #16 December 1997 sleeve explicitly lists Quake. Exact byte parity with that disc is not claimed. No modern source-code licence or later installer is substituted; no files stripped, modified or added."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c34dc7d506808e4514669c0d54069459ff4aed6167a4e8c16b17bf8034d2f74a
    size_bytes: 72157
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5924
    rights_holder: id Software / MacSoft / Lion Entertainment
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      underlying artwork retains its original copyright.
    notes: >-
      Exact 640x400 game surface cropped from original 800x600 native frame at
      (80,100)-(720,500), including HUD. Black emulator surround and Classic menu bar
      excluded; no resampling, colour changes or artwork edits. SHA-256
      c34dc7d506808e4514669c0d54069459ff4aed6167a4e8c16b17bf8034d2f74a, 72157 bytes. Native evidence only.
references:
- >-
  https://archive.org/download/macaddict-cd-016/MacAddict%20CD-ROM%20December%201997%2000.jpg
- https://github.com/benletchford/systemless/issues/5924
- https://github.com/benletchford/systemless/issues/3213
---

![Quake native gameplay capture](https://assets.systemless.org/catalogue/media/sha256/c3/c34dc7d506808e4514669c0d54069459ff4aed6167a4e8c16b17bf8034d2f74a.png)

Enter Gothic corridors, face monsters and search for the slipgates in this original
Macintosh preview of Quake. The complete 1.08.2 package retains its original
documents and limited preview levels.

Native map entry and turning input are verified. A startup console file-open
message remains before normal menu navigation. Browser input and performance,
combat, level completion and audio fidelity remain unverified. Launch stays disabled.
