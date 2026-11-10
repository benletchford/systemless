---
id: crystal-caliburn-demo
kind: game
title: Crystal Caliburn Demo
summary: >-
  Launch the ball and work the flippers in LittleWing's original Arthurian
  pinball demo.
developer: LittleWing
publisher: StarPlay Productions
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Arcade
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: d09fc36f native-equivalent runtime
    architecture: 68k
    environment: "Complete unchanged original 1.02 US StuffIt demo. Default CPU, initial display, partition, sound and preferences; ordinary clicks dismiss the original promotional, instructions and publisher screens. Documented Command-I and Command-N insert a coin and start a game. Dismiss the original new-game notices and wait for the ball to arrive in the chute. In a matched plunger pair, pre-input frames are RGB pixel-identical. Holding documented Shift for 60 ticks moves the plunger; release launches the ball into the playfield while omission leaves it in the chute. Both reach 2304 frontend / 2904 guest ticks with zero exhausted frames. A second matched pair holds documented Z and / for six ticks: both flippers visibly rise, while omission leaves them lowered; pre-input frames are RGB pixel-identical. Both reach 2316 frontend / 2916 guest ticks with zero exhausted frames. Actual captures inspected. No overrides, preference rewrites, patches, cheats or unlocks. These checks establish bounded native startup, ball launch and flipper response, not complete physics, nudge response, full-game behavior, persistence, audio fidelity, browser pacing or independent hardware accuracy. Frozen binary SHA-256 5821047ecb681850d77ae359361e3921a65bb10608ef50a402dbe1da6a7d9d5c has the same runtime tree as merged d09fc36f."
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5994
runtime:
  executable_path: Crystal Caliburn/Crystal Caliburn
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 0393aa37dcc5f5a2b065954866fbb2d3b15a37de001d3c494530b1abc7ca850b
    size_bytes: 1249417
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/crystal-caliburn
    - >-
      https://discmaster.textfiles.com/browse/20844/Mac%20Magazin%20and%20MacEasy%20Magazine%20CD%20-%20Issue%2001.iso/CD%20Extras/...%20mehr%20Spiele/Crystal%20Caliburn%E2%84%A2%20DEMO
    - https://github.com/benletchford/systemless/issues/5994
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: LittleWing; StarPlay Productions; original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This is project-policy acceptance, not an
      affirmative publisher licence grant. No applicable explicit redistribution
      prohibition was identified in the reviewed original notices. Preserve the entire
      unchanged archive, copyright notices and original demo restrictions; remove it if a
      rights holder objects.
    notes: >-
      The received 1249417-byte SIT is unmodified and contains one application.
      Independent unar agrees with production packing on the file, both forks and supported
      Finder type/creator/flags. Strict KPK end is 1799688 bytes, SHA-256
      d3cd55de2063d6ad318b664f99f424a8f59635a4a5f59f91945bd698b05b5bfe. All 635 resource payloads
      match the preserved Mac Magazin/MacEasy issue 1 demo by type/id/hash; both have
      empty data forks. Whole resource-fork bytes differ in metadata, period Finder
      metadata was not compared, and outer SIT byte identity with a period archive is not
      asserted. The received package includes no separate readme. Reviewed 59 selected
      UI/text/version resource records, with no compressed selected records, and inspected
      actual original promotional, instructions, publisher and gameplay screens. Some
      notices are bitmap artwork; complete extraction of all PICT text is not claimed.
      Archive and resource-format metadata and UI notices only; no guest instruction
      tracing, disassembly or game-script decoding. The original 90-second demo limit and
      disabled scoring remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 848c2fc9e36ea301bb7c997ade1c77a0b1110c3551e110a5a76dd2fe15adb72d
    size_bytes: 299800
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5994
    rights_holder: LittleWing; StarPlay Productions; original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted
      promotional demo; underlying artwork retains its original copyright.
    notes: >-
      Unscaled 616x516 gameplay crop (96,40)-(712,556) from the native flipper test,
      with the ball in play and both flippers raised. Excludes desktop, Classic Mac
      menu bar and window framing. No colour or artwork edits. 299800 bytes, SHA-256
      848c2fc9e36ea301bb7c997ade1c77a0b1110c3551e110a5a76dd2fe15adb72d. Native evidence only.
references:
- https://classicmacdemos.com/crystal-caliburn
- https://github.com/benletchford/systemless/issues/5994
---

![The original Crystal Caliburn demo with both flippers raised](https://assets.systemless.org/catalogue/media/sha256/84/848c2fc9e36ea301bb7c997ade1c77a0b1110c3551e110a5a76dd2fe15adb72d.png)

Click through the original introduction, then use **Command-I** to insert a coin
and **Command-N** to start. Dismiss the demo notices and wait for the ball.
Hold and release **Shift** to launch; **Z** and **/** operate the flippers.
The original demo limits play to 90 seconds and disables scoring. Browser launch
remains disabled pending qualification.
