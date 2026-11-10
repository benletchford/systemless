---
id: afterlife-demo
kind: game
title: Afterlife Demo
summary: Build gates for Heaven and Hell in LucasArts' original 1996 simulation demo.
developer: LucasArts
publisher: LucasArts
year: 1996
architectures:
- ppc
default_architecture: ppc
category: Simulation
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: d09fc36f native-equivalent runtime
    architecture: ppc
    environment: >-
      Complete unchanged five-file StuffIt demo, default CPU, initial display,
      partition, sound and preferences. Select the original PowerPC application, click
      normally past the logo, select Gates, choose the first Heaven gate, zoom in twice with
      the original magnifier button and place a gate on empty Heaven terrain. A matched
      placement-click omission pair has RGB-identical pre-input images. Active
      placement creates a visible gate and reduces the treasury from 8765432 to 8715432,
      matching its 50000 cost; control retains 8765432 without a gate. Both reach 1596
      frontend/guest ticks with zero exhausted frames. Actual captures inspected. The
      tutorial explains that the black terrain is astral space and the demo starts paused.
      The initial tiled LucasArts logo is transient; a normal click reaches the game
      world. This verifies bounded startup, selection, zoom and gate placement, not roads,
      zoning, population simulation, reincarnation, persistence, audio fidelity,
      browser controls/pacing or independent hardware accuracy. The 68040 application is
      preserved but unqualified. No emulator overrides, original-file edits, preference
      rewrites, cheats, unlocks or host extension installation. Frozen binary SHA-256
      5821047ecb681850d77ae359361e3921a65bb10608ef50a402dbe1da6a7d9d5c was built from
      679b14c; its runtime tree matches merged d09fc36f.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5990
runtime:
  executable_path: Afterlife/Afterlife Demo (PowerPC)
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: 84e5934a9b1121f335df8df8102cd75e14f8ca5f9141de2d6364630b1cb46657
    size_bytes: 16091905
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/afterlife
    - >-
      https://discmaster.textfiles.com/browse/17708/MacAddict_002_1996_10.iso/Demos/LucasArts/Afterlife%20Demo
    - https://github.com/benletchford/systemless/issues/5990
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: LucasArts; original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This is project-policy acceptance, not an
      affirmative publisher licence grant. No applicable explicit redistribution
      prohibition was identified in the reviewed original notices. Preserve the entire
      unchanged archive and all original notices and demo limitations; remove it if a rights
      holder objects.
    notes: >-
      The received 16091905-byte SIT is unmodified. Independent unar agrees with
      production packing on all five files, ten forks and five supported Finder
      type/creator/flags records. Strict KPK end is 35635249 bytes, SHA-256
      d7ab421c849758f4bd6509df8fe0ac9a661a4974aef2dcb902ac3d2c83977a31. Both bundled plain-text documents
      were read fully and 90 selected original UI/text/version resource records reviewed;
      no applicable restriction was found. The MacAddict 2 October 1996 disc preserves
      the period promotional demo. Its game data, PowerPC executable data and tutorial
      text match the received files. All 85 PowerPC and 82 68040 executable resource
      payloads match by type/id/hash; whole resource forks differ in metadata. The
      received Read Me adds a Control Strip/System 7.5.3 compatibility paragraph. Period
      document style forks were not compared. The disc also includes optional System Folder
      items outside this five-file web package; no complete-disc or outer-SIT byte
      identity is asserted. Every received web-package file, including both executables
      and both documents, is retained. Resource/format metadata and original UI notices
      only; no guest instruction tracing, disassembly or game-script decoding.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 402849f12af938292bdf59d98aa6336d70092b49ccf8397ded9bbce4fae13850
    size_bytes: 293682
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5990
    rights_holder: LucasArts; original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted
      promotional demo; underlying artwork retains its original copyright.
    notes: >-
      Unscaled 800x600 original game framebuffer after gate placement, including its
      own custom game menu and toolbar. No desktop, Classic Mac menu bar, emulator
      framing, cropping, colour changes or artwork edits. 293682 bytes, SHA-256
      402849f12af938292bdf59d98aa6336d70092b49ccf8397ded9bbce4fae13850. Native evidence only.
references:
- https://classicmacdemos.com/afterlife
- https://github.com/benletchford/systemless/issues/5990
---

![A Heaven gate placed in the original Afterlife demo](https://assets.systemless.org/catalogue/media/sha256/40/402849f12af938292bdf59d98aa6336d70092b49ccf8397ded9bbce4fae13850.png)

Click past the LucasArts logo to enter Heaven and Hell. The included
**Afterlife Demo Tutorial** explains gates, roads and zoning. The game starts
paused; Space pauses or resumes its clock. Native testing verifies gate
placement and its cost. Population simulation and browser play remain
unqualified, and browser launch stays disabled pending approval.
