---
id: prime-target-demo
kind: game
title: Prime Target Beta Demo
summary: >-
  Investigate a murder in Washington, D.C. in MacSoft’s original three-level
  action mystery demo.
developer: WizardWorks Group, Inc.
publisher: MacSoft
year: 1996
architectures:
- ppc
default_architecture: ppc
category: FPS
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 8ae270f0 runtime code
    architecture: ppc
    environment: >-
      Complete unchanged original beta demo and PowerPC executable, default CPU,
      partition, sound and initial display. Normal New Game and Return reach the rendered
      first level. The documented number 2 key changes the fist to a pistol and updates
      the HUD against an equal-duration no-input control; pre-input images match byte
      for byte. Both complete at 1326 frontend/guest ticks with zero automatic
      assertions and zero budget exhaustion. A separate Up-arrow 30-tick case produces an image
      identical to the no-input control, so forward movement is not verified. Actual
      captures inspected. This establishes startup and weapon selection only; combat,
      navigation, level completion, persistence, audio fidelity, independent hardware
      rendering and browser input/pacing remain unqualified. Frozen native binary SHA-256
      f43e3df889a5042e9b23ef0715ba81dd4cc6c5fa1951cdd25a46515fbe6ae84d has the same
      runtime code as 8ae270f0, with catalogue/docs differences only.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/5927
runtime:
  executable_path: Prime Target/Prime Target
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: caf75f8c529001fd7509989e87ee4c17ef391189c5fad1c0d8b98ef30154ab90
    size_bytes: 3964205
  provenance:
    redistribution: permitted
    original: true
    sources:
    - >-
      https://oldapplestuff.com/download/Macintosh/Macintosh_Garden/games/PrimeTarget_demo.sit
    - https://www.macintoshrepository.org/3308-prime-target
    - https://archive.org/details/mac-addict-s-games
    - https://github.com/benletchford/systemless/issues/5927
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: WizardWorks Group, Inc. / Bungie Software Products Corp. (licensed portions)
    permission: >-
      Accepted under the project promotional-demo policy with explicit maintainer
      authorization on 2026-11-10. Period cover-disc distribution is corroborated, the
      complete original package is preserved, and no explicit redistribution restriction
      was found in inspected applicable notices. This records project-policy acceptance,
      not an affirmative publisher licence grant. All original copyright notices
      remain intact; no modern engine licence is substituted.
    notes: >-
      Original 3964205-byte SIT, SHA-256
      caf75f8c529001fd7509989e87ee4c17ef391189c5fad1c0d8b98ef30154ab90; SHA1 a6765e5b74006c76a8c7293a96cb6afc02861e36 matches the
      independent archive listing. All 8 files, 16 forks and 8 Finder records match
      independent unar extraction; strict pack EOF 9625996, SHA-256
      f7352f7baacc0133e44bbfc0ee947539a09d3ea21e850c8473fd3b632acf0fb0. Full 3190-byte Readme, 77 selected
      text/UI/version resources and retained notice matches reviewed. Preserved 1997
      MacAddict Special Games HFS catalogue contains Prime Target Demo. Seven disc files
      inspected through verified HTTP 206 ranges; Readme, Shapes and Sounds data match,
      but executable/map/resources differ. No exact disc-byte/version parity or
      whole-disc hash is claimed. The beta retains three levels and omits movies, music and
      networking. No files stripped or added, no serial/unlock applied.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 03ede5e759ce47355016c033139f8cb923769302d914a5a0f18bffce45ecbad2
    size_bytes: 153161
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5927
    rights_holder: WizardWorks Group, Inc. / Bungie Software Products Corp. (licensed portions)
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      underlying artwork retains its original copyright.
    notes: >-
      640x480 game surface including HUD and selected pistol, cropped only from
      original 800x600 native frame at (80,60)-(720,540). No host UI, Classic menu bar,
      resampling, colour changes or artwork edits. SHA-256
      03ede5e759ce47355016c033139f8cb923769302d914a5a0f18bffce45ecbad2, 153161 bytes. Native evidence only; movement
      remains unverified.
references:
- https://archive.org/details/mac-addict-s-games
- https://www.macintoshrepository.org/3308-prime-target
- https://github.com/benletchford/systemless/issues/5927
---

![Prime Target native first-level capture](https://assets.systemless.org/catalogue/media/sha256/03/03ede5e759ce47355016c033139f8cb923769302d914a5a0f18bffce45ecbad2.png)

Investigate a senator’s murder through the corridors of Washington, D.C.
This original beta demo includes the first three levels of MacSoft’s action
mystery, using licensed Marathon 2 technology.

The first level renders and native weapon selection works. Forward movement
remains unverified after an input/control comparison, and browser play and
performance have not been qualified. Launch stays disabled.
