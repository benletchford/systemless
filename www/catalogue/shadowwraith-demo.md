---
id: shadowwraith-demo
kind: game
title: ShadowWraith Demo
summary: Pilot a cyberfighter through two maze-like levels in this original Macintosh action demo.
developer: Terminal Sunset Software
publisher: StarPlay Productions
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: 68k
    status: playable
    environment: >-
      Complete unchanged Info-Mac BinHex/Compact Pro archive at original default
      machine settings. Choose Change on the original 256-color prompt, Yes on
      the quit-other-applications prompt, then New Game. Holding the documented
      keypad 8 control moves the ship north off its starting pad. A matched
      omitted-input control stays on the pad. Pre-input RGB frames are identical;
      response frames differ by 1619 pixels and remain different after settling.
      Both runs finish 2964 frontend and 3564 guest ticks with zero exhausted frames
      and one observed-pixel assertion each. All seven captures in each case match
      the corresponding recovered period-disc run exactly. Actual screenshots inspected.
      Browser input/pacing, later levels, combat and audio fidelity remain unverified.
    evidence: https://github.com/benletchford/systemless/issues/6120
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: ppc
    status: boots
    environment: >-
      The original PowerPC path reaches an Initializing error notice asking to
      increase memory requirements after the normal color/application prompts.
      Cause remains unproven; no memory override was applied. Use the original
      68K path for the verified native interaction.
    evidence: https://github.com/benletchford/systemless/issues/6120
runtime:
  executable_path: ShadowWraith_Demo.cpt/ShadowWraith/ShadowWraith Demo
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mac/info-mac/game/arc/shadow-wraith-demo.hqx
    expected_sha256: 84eaf7a0545b6b55c80674345f2f7e342dbd1eec8bbbce3e064f56f5fd08eb81
    expected_size: 3314624
  provenance:
    redistribution: permitted
    original: true
    license: Maintainer-accepted promotional demo
    rights_holder: Terminal Sunset Software, StarPlay Productions and original contributors
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/shadow-wraith-demo.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    - https://archive.org/details/IMG33Dec1995
    - https://github.com/benletchford/systemless/issues/6120
    permission: >-
      Accepted under the existing project policy for complete, unchanged,
      period-distributed promotional demos, without claiming an affirmative
      publisher licence grant. No applicable explicit redistribution restriction
      was identified in the reviewed notices. Preserve the full archive, all
      files, credits and notices, two-level limit and disabled saving.
    notes: >-
      Complete preserved Info-Mac distribution. Its submission explicitly came
      from a third party who was not the author; no publisher upload or grant
      is inferred. The December 1995 Inside Mac Games cover disc independently
      preserves the demo: application data and all 396 application/game-media
      resource payloads, names, IDs and attributes match. Its README differs in
      whitespace/style and its outer folder icon differs; whole-disc or wrapper
      byte identity is not claimed. The full archived 6598-byte README and 53
      readable selected standard UI/text/version records were reviewed, with
      zero compressed unread records. Normal startup prompts and title credits
      were inspected. Production packing matches all seven files, fourteen forks
      and seven Finder records against independent unar extraction. No opaque
      game resources or guest instructions were decoded.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/shadowwraith-demo/screenshot.png
  provenance:
    redistribution: permitted
    content_only: true
    rights_holder: Terminal Sunset Software, StarPlay Productions and original contributors
    sources:
    - https://github.com/benletchford/systemless/issues/6120
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 640x480 crop of actual gameplay from the unchanged Info-Mac archive
      after moving north from the start pad. Excludes black outer framing and
      Classic Mac menu/desktop UI. Original game artwork remains unmodified.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- https://archive.org/details/IMG33Dec1995
- https://github.com/benletchford/systemless/issues/6120
---

## A cyberfighter in the maze

ShadowWraith's original two-level demo sends a fighter through a network of
rooms, keys, enemies and teleporters. This is the promotional demo, with saving
disabled, rather than the full retail game.

The original 68K application has verified native movement. At startup, choose
**Change** for 256 colors, confirm the application's normal prompt, then choose
**New Game**. The default keypad controls use **8** to move forward, **5** to
reverse, **4/6** to rotate and **7/9** to strafe. Browser play remains pending.

![ShadowWraith demo gameplay](incoming/shadowwraith-demo/screenshot.png)
