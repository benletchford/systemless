---
id: shadowwraith-demo
kind: game
title: ShadowWraith Demo
summary: >-
  Pilot a cyberfighter through two maze-like levels in this original Macintosh
  action demo.
developer: Terminal Sunset Software
publisher: StarPlay Productions
year: 1995
architectures:
- 68k
- ppc
default_architecture: 68k
category: Arcade
compatibility:
  status: unknown
  verified:
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: 68k
    environment: >-
      Complete unchanged Info-Mac BinHex/Compact Pro archive at original default
      machine settings. Confirm Yes on the original quit-other-applications prompt,
      click past the story screen, then choose New Game. Holding the documented keypad 8 control
      moves the ship north off its starting pad. A matched omitted-input control stays on
      the pad. Pre-input RGB frames are identical; response frames differ by 1619
      pixels and remain different after settling. Both runs finish 2964 frontend and 3564
      guest ticks with zero exhausted frames and one observed-pixel assertion each. All
      seven captures in each case match the corresponding recovered period-disc run
      exactly. Actual screenshots inspected. Browser input/pacing, later levels, combat
      and audio fidelity remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6120
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: 0a0c6a31 native
    architecture: ppc
    environment: >-
      The original PowerPC path reaches an Initializing error notice asking to
      increase memory requirements after the normal color/application prompts. Cause remains
      unproven; no memory override was applied. Use the original 68K path for the
      verified native interaction.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/6120
  - date: "2026-11-12"
    tester: Catalogue maintainer
    systemless_version: bc789543 private browser
    architecture: 68k
    status: boots
    environment: >-
      Chromium worker/WebGL at default settings. Confirm the quit-applications
      prompt, click past the story, then New Game. Keypad 8 moves the ship north;
      the observed start-pad pixel check passes and the response screenshot was
      inspected. The 20-second interaction records 24.21 guest ticks/s, below the
      required 50, despite 62.11 host FPS, 20.6 ms maximum frame and 491.7 ms
      minimum audio queue. Zero action/lifecycle/network errors. The sustained
      phase and idle browser control remain unqualified. The original archive
      and both page/worker Wasm response bodies are pinned by size and SHA-256.
      Only the exact promoted entry and launch flag were added to the private
      preview; no game/default changes or concurrent heavy local job. Launch
      remains disabled pending pacing qualification.
    evidence: https://github.com/benletchford/systemless/issues/6123
runtime:
  executable_path: ShadowWraith_Demo.cpt/ShadowWraith/ShadowWraith Demo
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 84eaf7a0545b6b55c80674345f2f7e342dbd1eec8bbbce3e064f56f5fd08eb81
    size_bytes: 3314624
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/shadow-wraith-demo.hqx
    - https://ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
    - https://archive.org/details/IMG33Dec1995
    - https://github.com/benletchford/systemless/issues/6120
    license: Maintainer-accepted promotional demo
    rights_holder: Terminal Sunset Software, StarPlay Productions and original contributors
    permission: >-
      Accepted under the existing project policy for complete, unchanged,
      period-distributed promotional demos, without claiming an affirmative publisher licence
      grant. No applicable explicit redistribution restriction was identified in the
      reviewed notices. Preserve the full archive, all files, credits and notices, two-level
      limit and disabled saving.
    notes: "Complete preserved Info-Mac distribution. Its submission explicitly came from a third party who was not the author; no publisher upload or grant is inferred. The December 1995 Inside Mac Games cover disc independently preserves the demo: application data and all 396 application/game-media resource payloads, names, IDs and attributes match. Its README differs in whitespace/style and its outer folder icon differs; whole-disc or wrapper byte identity is not claimed. The full archived 6598-byte README and 53 readable selected standard UI/text/version records were reviewed, with zero compressed unread records. Normal startup prompts and title credits were inspected. Production packing matches all seven files, fourteen forks and seven Finder records against independent unar extraction. No opaque game resources or guest instructions were decoded."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 75fe4669e599e411506699e9fe993782e787bbe54b91d711156203c93182f2c2
    size_bytes: 187564
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6120
    rights_holder: Terminal Sunset Software, StarPlay Productions and original contributors
    permission: Fresh content-only Systemless capture of the project-policy-accepted demo.
    notes: >-
      Unscaled 640x480 crop of actual gameplay from the unchanged Info-Mac archive
      after moving north from the start pad. Excludes black outer framing and Classic Mac
      menu/desktop UI. Original game artwork remains unmodified.
references:
- https://ftp.funet.fi/pub/mac/info-mac/game/arc/00arc-abstracts.txt
- https://archive.org/details/IMG33Dec1995
- https://github.com/benletchford/systemless/issues/6120
---

## A cyberfighter in the maze

ShadowWraith's original two-level demo sends a fighter through a network of
rooms, keys, enemies and teleporters. This is the promotional demo, with saving
disabled, rather than the full retail game.

The original 68K application has verified native movement. Confirm its normal
quit-other-applications prompt, click past the story screen, then choose
**New Game**. The PowerPC path also asks to change to 256 colors before its
initialization error. The default keypad controls use **8** to move forward,
**5** to reverse, **4/6** to rotate and **7/9** to strafe. Browser play remains
pending.

![ShadowWraith demo gameplay](https://assets.systemless.org/catalogue/media/sha256/75/75fe4669e599e411506699e9fe993782e787bbe54b91d711156203c93182f2c2.png)
