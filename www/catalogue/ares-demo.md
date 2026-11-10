---
id: ares-demo
kind: game
title: Ares Demo
summary: >-
  Pilot a starship and command a fleet in the original 1997 Macintosh demo of
  Nathan Lamont's space action and strategy game.
developer: Nathan Lamont
publisher: Changeling Software
year: 1997
architectures:
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: playable
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: aa42fa69 native-equivalent runtime
    architecture: ppc
    environment: >-
      Complete unchanged original 1.0.1 demo ZIP, default CPU, initial display,
      partition, sound and preferences. Select the exact game application, accept the
      original Continue choice for unavailable optional networking, let the title finish,
      press Space, choose Begin, accept chapter one and its mission analysis. The mission
      interface and starfield render. The original F1 help documents F9 for normal 1:1
      zoom, keypad 6 to turn clockwise, keypad 8 for thrust and Option for weapon one.
      After the common F9 choice, hold those keys for 60, 90 and 60 ticks versus
      matched omission. Both cases reach 2358 frontend/guest ticks with zero exhausted
      frames. Shared pre-input closeups are RGB pixel-identical; active turning changes ship
      orientation, the combined turn/thrust sequence changes the camera/background, and
      firing produces two visible projectiles and reduces ammunition from 158 to 146.
      Omission retains 158 and produces no projectiles. Actual captures inspected. The
      control has natural background motion; zero absolute scene motion is not
      asserted. An earlier thrust-only pair at the initial distant view is pixel-identical and
      is retained as inconclusive, as is an isolated initial-trajectory thrust pair at
      1:1 zoom. A further matched pair first brakes normally with keypad 5 for 120
      ticks and settles for 30 ticks in both cases. Holding keypad 8 for 90 ticks then
      moves the active camera/starfield; omission remains RGB pixel-identical through that
      period and a further 60-tick coast. Shared pre-input frames are identical; after
      acceleration 2713 pixels differ. Both cases reach 2508 frontend/guest ticks with
      zero exhausted frames. This verifies bounded acceleration from the normally
      braked state. These tests verify bounded native controls, not full combat, mission
      completion, persistence, multiplayer, audio fidelity, browser pacing or independent
      hardware accuracy. Native binary SHA-256
      10d5f86ffab27594352fafb4a43b78ad3a7f8c2b1b276645e8e3752d3eda4a3f. Runtime code from 6be32c91 matches the tested tree
      merged as aa42fa69 in ZIP loader fix 5979. No emulator overrides, original-file
      edits, cheats, unlocks, preference rewrites or host extension installation.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5980
runtime:
  executable_path: Ares Demo 1.0.1 ƒ/Ares Demo
artifacts:
- id: archive
  role: archive
  format: zip
  source:
    type: sha256
    sha256: 643e53a7ba0a21e78eef76de5ab2389fa44f9761117c48cfca76719e63fcc95c
    size_bytes: 2481511
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://arescentral.org/about/versions
    - https://downloads.arescentral.org/Ares/Ares-1.0.1-Demo.zip
    - https://groups.google.com/g/muc.archive.update/c/MDkEUTCw1p8
    - https://github.com/benletchford/systemless/issues/5980
    license: Maintainer-accepted freely distributed promotional demo
    rights_holder: Nathan Lamont; Changeling Software; original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      freely circulated promotional demos. The period Info-Mac mirror listing records
      ares-demo-101.hqx dated 8 October 1997; Ares Central preserves the public demo and
      its release history. This policy acceptance is not an affirmative publisher
      licence grant. No applicable explicit redistribution prohibition was identified in
      the reviewed original notices. Preserve the exact complete archive and all original
      notices, and remove it if a rights holder objects.
    notes: >-
      Original ZIP SHA-256
      643e53a7ba0a21e78eef76de5ab2389fa44f9761117c48cfca76719e63fcc95c, 2481511 bytes. Modern ZIP wrapper byte identity with the period HQX is
      not asserted. Four bundled plain-text documents were reviewed; 186 selected UI/text
      resource records were scanned, all relevant notice contexts were reviewed, and a
      whole-package notice search was examined. A legacy DITL 700 is explicitly titled
      Pre-release Registration and asks for a tester ID with a confidentiality
      warning. It remains unmodified. Treating that as pre-release testing UI, rather than a
      restriction on this public demo, is a project-policy inference supported by the
      period public release, the original 1.0.1D version metadata, registration-free
      mission startup and the original version history's no-expiration statement. No
      expiration patch is applied. Original demo chapter and ten-minute limits remain.
      Preserve the NetSprocket library, eDOC portrait reader, ResExp and all other notices;
      no host extensions are installed. The later partial GPL/Creative Commons releases
      are not asserted to license this complete original archive. Independent
      ZIP/AppleDouble parsing matches production packing for all eleven original files, all 22
      forks and eleven supported type/creator/flags records. Strict KPK end is 5089198
      bytes, SHA-256 13e8c4ee9ff7a668cbc766b419f495036483c65bae02567f4c4c4c0c98adca90.
      Resource metadata and notice inspection only; no guest instruction tracing,
      disassembly or game-script decoding.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c0b1952072841e1005c0b1b4a7a59cf5e2fd3c12f405ff0c13e7c5e7b68c66f1
    size_bytes: 121049
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5980
    rights_holder: Nathan Lamont; original contributors
    permission: >-
      Fresh content-only capture of the unchanged promotional demo accepted under
      project policy; the underlying artwork retains its original copyright.
    notes: >-
      Exact unscaled 800x600 native fullscreen game framebuffer at the documented
      normal 1:1 zoom after turning, thrust and firing. No host interface, desktop, menu
      bar, cropping, colour changes or artwork edits. Native evidence only.
references:
- https://arescentral.org/about/versions
- https://github.com/benletchford/systemless/issues/5980
---

![Ares Demo after turning, thrust and firing](https://assets.systemless.org/catalogue/media/sha256/c0/c0b1952072841e1005c0b1b4a7a59cf5e2fd3c12f405ff0c13e7c5e7b68c66f1.png)

Continue past the optional networking notice to play alone. Let the title
finish, press Space, choose Begin and follow the original mission screens.
Press F9 for the normal 1:1 view. Use keypad 8 to accelerate, keypad 5 to slow
down, keypad 4 and 6 to turn, and Option to fire. F1 shows the original controls.
Tab engages warp and Delete clears messages.

This is the original limited 1997 demo, with its original chapter and time
limits intact. Native PowerPC testing verifies bounded turning, thrust and
firing. Browser launch and public qualification are pending.
