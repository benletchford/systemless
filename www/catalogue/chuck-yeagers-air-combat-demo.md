---
id: chuck-yeagers-air-combat-demo
kind: game
title: Chuck Yeager's Air Combat Demo
summary: >-
  Fly a P-51 Mustang in Electronic Arts' original two-minute Macintosh flight
  demo.
developer: Brent Iverson / Looking Glass Technologies / Mark Adams
publisher: Electronic Arts
year: 1993
architectures:
- 68k
default_architecture: 68k
category: Simulation
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: d09fc36f native-equivalent runtime
    architecture: 68k
    environment: >-
      Complete unchanged original six-file StuffIt demo. Default CPU, initial
      display, partition, sound and preferences; accept the original recommended Medium window
      choice, dismiss the title and welcome, and use Fly > Test Flight or Create
      Mission. No emulator overrides, preference rewrites, patches, cheats or unlocks. A
      matched Test Flight pair presses R normally in both cases, then sends the number-row
      0 key in the active case only. Shared pre-input captures are RGB
      pixel-identical. Active throttle changes from 0% to 50% and speed from 0 to 2 mph; omission
      retains 0% and 0 mph. Both reach 3036 frontend / 3575 guest ticks with zero exhausted
      frames. This does not establish the key's full throttle range. The normal
      default Create Mission scenario starts the airborne P-51 against a B-17. A second
      matched pair holds documented Space for 60 ticks versus omission. Shared pre-input
      captures are RGB pixel-identical. Active ammunition falls from 2400 to 2375 and
      bullet streaks appear; control retains 2400 without those streaks. Both reach 2664
      frontend / 3202 guest ticks with zero exhausted frames. Actual captures inspected.
      These tests verify bounded native startup, throttle response and firing, not
      steering, enemy AI, damage, full combat, mission completion, persistence, audio
      fidelity, browser pacing or independent hardware accuracy. The Help topic popup opens
      but the tested selections leave Using Help displayed; help-topic navigation
      remains unverified. Frozen binary SHA-256
      5821047ecb681850d77ae359361e3921a65bb10608ef50a402dbe1da6a7d9d5c was built from 679b14c; its runtime tree matches merged
      d09fc36f.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5988
runtime:
  executable_path: Air Combat Demo/Air Combat Demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 5bc43e6870aba7998315bdfd386cdef9d20ce5e869bb96fc123bed868b0ad505
    size_bytes: 1271878
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/chuck-yeagers-air-combat
    - >-
      https://discmaster.textfiles.com/browse/20851/Mac%20Magazin%20and%20MacEasy%20Magazine%20CD%20-%20Issue%2010.iso/CD%20Extras/Flugsimulatoren/Flugsimulatoren/Air%20Combat%20Demo
    - >-
      https://discmaster.textfiles.com/view/11579/IMG%20Dec%2094.toast/Demos/Air%20Combat%20Demo/Air%20Combat%20Demo%20Instructions
    - https://github.com/benletchford/systemless/issues/5988
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Electronic Arts; original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This is project-policy acceptance, not an
      affirmative publisher licence grant. No applicable explicit redistribution
      prohibition was identified in the reviewed original notices. Preserve the entire
      unchanged archive, copyright notices and original demo restrictions; remove it if a
      rights holder objects.
    notes: >-
      The received 1271878-byte SIT is unmodified. Independent unar agrees with
      production packing on all six files, twelve forks and six supported Finder
      type/creator/flags records. Strict KPK end is 2583662 bytes, SHA-256
      2e4263297c6fb24bfe632613198ee913c0ce9bd731b868f098d7e99a4de1fae2. All twelve file forks are also
      byte-identical to the complete six-file demo folder on the preserved Mac Magazin/MacEasy
      10 disc, including Pilot Data and Last Film. Outer SIT wrapper byte identity
      with a period archive is not asserted; period Finder metadata was not compared. The
      separate IMG December 1994 instructions were fully reviewed and describe the
      public limited demo, but label it 1.0.1; the tested app reports 1.0. That separate
      document is not silently inserted into the six-file package. Selected 120 original
      UI/text/version records were searched for notices and the startup/title/demo
      prompts inspected; no compressed selected records or notice restrictions were found.
      Some PICT resources are bitmaps or unsupported by the diagnostic text parser, so
      complete PICT text extraction is not claimed. Inspection covers archive/resource
      metadata and UI notices only, with no guest instruction tracing, disassembly or
      game-script decoding. The original two-minute flight limit and disabled
      campaign/historic missions remain intact.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 8bf3fe87631de195a3bc01ea5a54b88cb712e6898e44eb6d6252245442fff170
    size_bytes: 81819
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5988
    rights_holder: Electronic Arts; original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted
      promotional demo; underlying artwork retains its original copyright.
    notes: >-
      Exact unscaled 512x360 cockpit crop from the active firing frame,
      (144,130)-(656,490), excluding desktop, black framing and the Classic Mac menu bar. No artwork
      or colour edits. 81819 bytes, SHA-256
      8bf3fe87631de195a3bc01ea5a54b88cb712e6898e44eb6d6252245442fff170. Native evidence only.
references:
- https://classicmacdemos.com/chuck-yeagers-air-combat
- https://github.com/benletchford/systemless/issues/5988
---

![P-51 cockpit firing in the original demo](https://assets.systemless.org/catalogue/media/sha256/8b/8bf3fe87631de195a3bc01ea5a54b88cb712e6898e44eb6d6252245442fff170.png)

Choose the recommended **Medium** window size, dismiss the title and welcome,
then choose **Test Flight** or **Create Mission** from the Fly menu. The original
demo limits each flight to two minutes and leaves campaigns and historic
missions unavailable.

Space fires the selected weapon. Esc pauses and opens the menus; R resumes.
Native testing verifies bounded throttle response and firing. Browser play
remains disabled pending control and pacing approval.
