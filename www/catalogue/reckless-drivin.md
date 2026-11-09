---
id: reckless-drivin
kind: game
title: Reckless Drivin'
summary: >-
  Race through traffic in Jonas Echterhoff's top-down Macintosh arcade racer.
developer: Jonas Echterhoff
publisher: Jonas Echterhoff
year: 2000
architectures:
- ppc
default_architecture: ppc
category: Arcade
launch_enabled: false
compatibility:
  status: unknown
  verified:
  - date: "2026-11-10"
    tester: Catalogue maintainer
    systemless_version: 91245206 native candidate
    architecture: ppc
    environment: >-
      Complete unchanged original Reckless Drivin' 1.1 shareware package and exact
      executable, PowerPC, default 25 MHz, partition, sound and initial display.
      Original trial button and Start enter the road; DrawSprocket selects and
      reserves 640x480 at depth 8. Ordinary Keypad8 throttle advances the speedometer
      needle and changes the road view. A matched omitted-throttle control retains
      zero speed. Pre-input images match exactly. Each asserted repeat passes four
      measured pixels and five strict own-run PNG comparisons at 796 frontend/guest
      ticks, zero budget exhaustion. Own-run references check consistency, not
      independent hardware accuracy. The native executable has the same full source
      tree as this runtime candidate, ece6293a7fc9c0fd5c137f153b10379ede28540e;
      binary SHA-256 f43e3df889a5042e9b23ef0715ba81dd4cc6c5fa1951cdd25a46515fbe6ae84d.
      This requires the InputSprocket time conversion in #5913 and reservation
      buffering fix in #5915. Pink sprite-edge/trailing pixels remain visible;
      rendering fidelity needs independent review. Steering, braking, collisions,
      scoring, completed levels, persistence, audio fidelity, browser input/pacing
      and released public play remain unqualified. Launch stays disabled.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/5916
runtime:
  executable_path: "Reckless Drivin'/Reckless Drivin' 1.1"
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/reckless-drivin/original.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://oldapplestuff.com/download/Macintosh/Macintosh_Garden/games/Reckless_Drivin_1.1.sit
    - https://www.macintoshrepository.org/4957-reckless-drivin-
    - https://github.com/benletchford/systemless/issues/5916
    rights_holder: Jonas Echterhoff
    license: Original complete-package redistribution grant
    permission: >-
      The bundled Readme permits free distribution when all files from the original
      package are included. Modified files and redistribution of serial numbers are
      prohibited. Preserve the whole original package and its notices. Register's
      STR#928 separately permits free distribution without modification. No serial
      number, crack or author-published unlock code is applied or redistributed.
    notes: "Original 5597681 bytes, SHA-256 e7fa99614fd02e9c40b14cec181a064908a57970bde8e5c2c3670ef446e97b9f; SHA1 1634112d9cfc079fd6239933fc6de40deb036675 independently matches the archive listing. All 5 files, 10 forks and 5 Finder records match independent unar extraction; strict web-pack end 6241772, SHA-256 28afda3c11076bd5848161bed3212e30963dac536ca2c60bfb1103fce243e8f8. The Readme heading retains v1.0, but its history and the executable version resource identify 1.1. No modern-port licence is substituted. The separate 1.44 package is not used."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/reckless-drivin/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5916
    rights_holder: Jonas Echterhoff
    permission: >-
      Fresh Systemless capture from the permitted unchanged shareware package.
      The underlying game artwork remains Jonas Echterhoff's copyright.
    notes: "Exact 640x480 native game surface with road, car and speedometer after throttle input; no host UI, Classic Mac menu bar, resampling, palette editing or other artwork changes. SHA-256 46083e582948b2daa0473aef2f9b4653d2a636aca6136eb18f68c0108af896af, 536333 bytes. Pink sprite-edge pixels are preserved and flagged for review; this is not browser or rendering-fidelity qualification."
references:
- https://jonasechterhoff.com/Reckless_Drivin.html
- https://github.com/benletchford/systemless/issues/5916
---

![Reckless Drivin' native driving capture](incoming/reckless-drivin/gameplay.png)

Drive through traffic, skid around bends and try to reach the end of each road.
This entry preserves the original 1.1 shareware package, including its Readme and
registration helper. Its first-three-level trial remains intact.

Native throttle input is verified. Pink sprite-edge pixels still need rendering
review, and browser play and performance remain unverified. Launch stays disabled.
