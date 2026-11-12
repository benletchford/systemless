---
id: battle-girl-demo
kind: game
title: Battle-Girl Demo
summary: Defend Battle-Girl's machine with directional weapons in the original six-level Macintosh demo.
developer: Ultra/United Games
publisher: Ultra/United Games
year: 1997
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: ppc
    status: playable
    environment: >-
      Complete unchanged original SIT at default machine settings. Select Play
      from the original title menu, then hold the documented W key for 60 ticks.
      Yellow primary projectiles appear; a matched omitted-W control has none.
      Checked pre-input RGB frames match exactly and response frames differ by
      566 pixels. Both runs finish 1448 frontend and guest ticks, zero exhausted
      frames and one observed projectile-pixel assertion each. Actual response
      captures inspected. No memory, CPU, display, preference or game-file
      modifications. Mouse movement, later levels, audio fidelity and browser
      input/pacing remain unverified.
    evidence: https://github.com/benletchford/systemless/issues/6129
runtime:
  executable_path: battle-girl DEMO folder/battle-girl DEMO
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: url
    url: https://download.classicmacdemos.com/battle-girl.sit
    expected_sha256: 07fafefccf87eaaccea627926cb8c01031a5ea36a61d6b16a98b9fe3287c31da
    expected_size: 1378744
  provenance:
    redistribution: permitted
    original: true
    license: Publisher demo redistribution permission
    rights_holder: Ultra/United Games, Peter N Lewis and original contributors
    sources:
    - https://classicmacdemos.com/battle-girl
    - https://static.classicmacdemos.com/demos/battle-girl/README.txt
    - https://www.vintageapplemac.com/files/misc/MacWorld_UK_Jul_2000.pdf
    - https://github.com/benletchford/systemless/issues/6129
    permission: >-
      The bundled publisher README expressly permits free redistribution of this
      demonstration version while it remains unaltered and includes that README.
      Preserve the complete four-file package, all credits, notices and original
      demo limits. The bundled Register 1.2.5 utility separately permits free
      distribution without modification in its STR# 928 notice. No full-game or
      broader third-party licensing grant is claimed.
    notes: >-
      Macworld UK July 2000, printed page 16, advertises the same six-level demo
      with levels 1, 2, 3, 5, 10 and 40, matching the bundled README. Its actual
      cover-disc listing was rendered and inspected. This verifies period demo
      distribution; whole period-wrapper or fork byte identity is not claimed.
      The recovered MacAddict 17 full trial is a different larger package and
      was not substituted for this demo. The complete 4095-byte README and
      9028-byte ordering notice plus 73 readable selected standard UI/text/version
      records were reviewed, with zero compressed unread records. The original
      title and first level were inspected. Production packing independently
      matches all four files, eight forks and four Finder records against unar.
      Retain the original six levels, single music track, 8-bit audio and disabled
      auxiliary features. Opaque game data and guest instructions were not decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/battle-girl-demo/screenshot.png
  provenance:
    redistribution: permitted
    content_only: true
    rights_holder: Ultra/United Games and original contributors
    sources:
    - https://github.com/benletchford/systemless/issues/6129
    permission: >-
      Fresh real gameplay capture of the redistributable demo accepted under the
      project's promotional-demo screenshot policy. Underlying artwork remains
      the property of its original rights holders; no broader artwork licence is claimed.
    notes: >-
      Unscaled 800x600 original game content and HUD from the exact unchanged SIT
      tested, while firing the primary weapon. The complete native frame contains
      no host framing or Classic Mac menu bar. No pixels were retouched.
references:
- https://classicmacdemos.com/battle-girl
- https://www.vintageapplemac.com/files/misc/MacWorld_UK_Jul_2000.pdf
- https://github.com/benletchford/systemless/issues/6129
---

## Six programs against Terminus

Battle-Girl's original Macintosh demo includes levels **1, 2, 3, 5, 10 and 40**.
It retains one music track, 8-bit sound and the original demo's feature limits.
The complete package includes the publisher's README, ordering information and
Register utility.

Choose **Play** from the title screen. The original controls use the mouse for
movement and the keyboard for independent weapon directions: **W/A/S/D** and
**Q/E/Z/C**. **Space** fires an acquired secondary weapon. Native primary firing
is verified; browser play remains pending.

![Battle-Girl demo gameplay](incoming/battle-girl-demo/screenshot.png)
