---
id: lode-runner-2-demo
kind: game
title: Lode Runner 2 Demo
summary: >-
  Collect gold and avoid the monks in MacSoft’s original isometric Lode Runner 2
  demo.
developer: Presage Software, Inc.
publisher: MacSoft
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Puzzle
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 202c39f6 runtime code
    architecture: ppc
    environment: >-
      Complete unchanged original demo, default PowerPC, CPU, initial display,
      partition and sound. Normal Single Player, New and Start Game reach the level with
      default DEMO player. Read-only control panel confirms Pad 3 movement. Holding Pad 3
      for 60 ticks and releasing for 12 moves the runner against an equal-duration
      no-input control; pre-input captures match byte for byte. Both finish at 1608
      frontend/guest ticks with two measured pixel assertions each and zero exhausted frames.
      Actual captures inspected. This verifies startup and bounded movement only;
      collection, completion, persistence, audio fidelity, independent hardware rendering and
      browser input/pacing remain unqualified. Frozen native binary SHA-256
      7641c54678bc90326ca29118a73a1936f5ccbde9a11cdc8c2123979cd2937514.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5934
runtime:
  executable_path: LR2 Demo/LR2 Demo
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 45276cabeac9e3c4610096ac97e96a4d0f2eccde5f6134161ce1f5ab11a2a2f7
    size_bytes: 11032735
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/lode-runner-2
    - https://www.vintageapplemac.com/files/misc/9903.pdf
    - https://github.com/benletchford/systemless/issues/5934
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Presage Software, Inc. / MacSoft
    permission: >-
      Accepted under the project promotional-demo policy with explicit maintainer
      authorization on 2026-11-10. Period cover-disc distribution is corroborated, the
      complete original package is preserved, and no explicit redistribution restriction
      was found in inspected applicable notices. This records project-policy acceptance,
      not an affirmative publisher licence grant. All original notices remain intact.
    notes: >-
      Original 11032735-byte SIT; all 22 files, 44 forks and 22 Finder records match
      independent unar extraction. Strict expanded KPK SHA-256
      61898863aab88e2ad01b8cfce556ad6f7c584695cb7187445a27da25efc3b2d9, 22275712 bytes. Macworld March 1999
      printed page 60 explicitly advertises this title’s demo on its cover CD; actual scan
      inspected. No exact cover-disc/version parity is claimed. Independent demo
      mirror matches every data fork, including HiScores, and all 106 application resource
      payloads; raw resource-fork metadata differs and mirror adds Config. Current
      mirror archive does not match its repository listing hash; no whole-archive parity
      claimed. Thirty UI/text/version resources, retained notice matches and the normal
      About panel reviewed; package contains no ReadMe/licence file. Original HiScores
      retained, no reset, configuration change, unlock or files added/removed.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 69268aa61ff3ce6bbc0eb22671c8b5572fa7a71355d1d7d85f7c394100afdd38
    size_bytes: 159749
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5934
    rights_holder: Presage Software, Inc. / MacSoft
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      underlying artwork retains its original copyright.
    notes: >-
      640x480 content surface, including the runner after default Pad 3 movement and
      HUD, cropped only from native 800x600 frame at (80,70)-(720,550). No menu bar,
      desktop, resampling, colour changes or artwork edits. SHA-256
      69268aa61ff3ce6bbc0eb22671c8b5572fa7a71355d1d7d85f7c394100afdd38, 159749 bytes. Native evidence only.
references:
- https://classicmacdemos.com/lode-runner-2
- https://global.tozaigames.com/loderunner/history
- https://www.vintageapplemac.com/files/misc/9903.pdf
- https://github.com/benletchford/systemless/issues/5934
---

Collect the gold while avoiding the monks in this original Macintosh demo of
Lode Runner 2, the isometric sequel from Presage and MacSoft.

Choose **Single Player → New → Start Game** to enter the default level. Movement
uses numeric keypad **7, 9, 1 and 3**; the original Options → Configure Controls
panel displays the other default bindings. Ordinary arrow keys are not bound to
movement by default.

![Lode Runner 2 demo gameplay](https://assets.systemless.org/catalogue/media/sha256/69/69268aa61ff3ce6bbc0eb22671c8b5572fa7a71355d1d7d85f7c394100afdd38.png)

The complete promotional demo is preserved unchanged, including its original
score table. Native startup and bounded Pad 3 movement have been checked.
Browser launch remains disabled pending browser controls and pacing approval.
