---
id: avernum-3-demo
kind: game
title: Avernum 3 Demo
summary: >-
  Lead four adventurers from Fort Emergence to investigate the surface world in
  Spiderweb Software's original fantasy role-playing demo.
developer: Jeff Vogel
publisher: Spiderweb Software
year: 2002
architectures:
- ppc
default_architecture: ppc
category: Role-Playing
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-09"
    tester: Catalogue maintainer
    systemless_version: 6513c6a4 private native source; browser qualification pending
    architecture: ppc
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5895
    environment: >-
      Complete original classic Macintosh v1.0.1 demo installer from the publisher.
      PowerPC PEF application with cfrg and no CODE resources. Default PPC 25 MHz,
      original display choice, preferred partition, Normal difficulty, default
      four-character party and sound retained. Normal intro and Instant Help lead
      to the Guest Quarters in Fort Emergence. A terrain click after shared
      left-key setup moves the party; the equal-clock omitted-click control
      retains its position. Fresh native repeats each pass nine checks at 3148
      frontend and guest ticks, with zero exhaustion. Actual captures inspected;
      these checks are not independent hardware-oracle or isolated-keyboard proof.
      Requires the working-directory resource fix merged in PR 5874. Browser
      input, rendering and pacing, combat, complete quests, persistence and audio
      fidelity remain unverified. No registration, editor, date, game-byte or
      partition changes. The sibling Avernum pacing hold does not qualify this
      title; its own hosted-browser replay remains required.
runtime:
  executable_path: Avernum 3 v1.0.1 Demo Installer/Avernum 3 ƒ v1.0.1/Avernum 3 v1.0.1
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: url
    url: https://www.spiderwebsoftware.com/ftp/mac/Avernum3.demo.v101.bin
    expected_sha256: f4333188152538ff4f8f5276c8a792ec162eaa3c238ad90d0489005d36f65a1e
    expected_size: 11975936
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://www.spiderwebsoftware.com/avernum/avernum3old/index.html
    - https://www.spiderwebsoftware.com/ftp/mac/Avernum3.demo.v101.bin
    - https://github.com/benletchford/systemless/issues/5895
    license: Complete bundled Spiderweb Software License
    rights_holder: Spiderweb Software, Inc. and its licensors
    permission: >-
      Section 1 covers the application, demonstration and other accompanying
      software, including licensors' interests. Section 2 permits non-profit
      distribution without prior written notice only if the software is
      unmodified and its complete works are included. Preserve this complete
      original installer and every bundled component and notice, the unregistered
      demo state and the original thirty-day condition. No standalone component
      licence or registration, date or restriction bypass is claimed.
    notes: >-
      Publisher-linked classic Macintosh v1.0.1 MacBinary demo, 11975936 bytes,
      SHA-256 f4333188152538ff4f8f5276c8a792ec162eaa3c238ad90d0489005d36f65a1e.
      A fresh publisher download matches the preserved original byte for byte.
      The full 7737-byte bundled licence, 961-byte documentation note and 3957-byte
      order form were read. Notices and Arrival instructions in the 59-page
      manual were inspected; a complete manual reading is not claimed.
      Production extraction retains all thirteen records, including game/data,
      licence, manual/order materials and bundled DrawSprocketLib, with strict
      end-of-pack validation. Independent outer extraction does not establish
      parity of the expanded VISE payload. The original archive is neither
      stripped nor repacked; the permission claim concerns the complete supplied
      demo, not individual bundled components or registered content.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/avernum-3-demo/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5895
    permission: >-
      Original Systemless gameplay capture of the unregistered demo; underlying
      artwork remains the property of Spiderweb Software and its licensors.
    notes: >-
      Native source 6513c6a4, default party in the Guest Quarters at Fort Emergence
      after a terrain click. Lossless 780x579 crop removes only black desktop
      margins from the 800x600 capture. The full game interface is retained;
      no retouching or resampling. This native image is not a browser approval.
references:
- https://www.spiderwebsoftware.com/avernum/avernum3old/index.html
- https://github.com/benletchford/systemless/issues/5895
---

![Avernum 3's default party in Fort Emergence](incoming/avernum-3-demo/gameplay.png)

The original 2002 adventure begins in the Guest Quarters of Fort Emergence.
Create a party, explore the surface and investigate the plagues threatening its
towns. The unregistered demo contains the first quarter of the adventure and
retains its original shareware conditions.

This classic Macintosh v1.0.1 package has bounded native party-movement evidence.
Launch remains disabled while hosted-browser input, rendering and pacing are
qualified.
