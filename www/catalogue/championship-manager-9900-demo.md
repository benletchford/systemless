---
id: championship-manager-9900-demo
kind: game
title: Championship Manager 99/00 Demo
summary: >-
  Take charge of a football club, review squad news and manage the coming season
  in the original Macintosh demo.
developer: Sports Interactive
publisher: Eidos Interactive
year: 1999
architectures:
- ppc
default_architecture: ppc
category: Strategy
compatibility:
  status: unknown
  verified:
  - date: "2026-11-11"
    tester: Catalogue maintainer
    systemless_version: df645359 native-equivalent runtime
    architecture: ppc
    environment: >-
      Complete unchanged demo with original guest defaults and normal English
      Leagues quick start. Taking control of Arsenal reaches the manager news
      screen. Continue Game selects board expectations; the omitted-input control
      remains on the appointment message. Both runs pass six pixel assertions at
      13876 frontend and guest ticks with zero exhausted frames. RGB-normalized
      pre-input captures match exactly; post-input captures differ at 28362
      pixels. Actual captures inspected. This verifies bounded native news
      navigation only. Date advancement, matches, later management actions,
      persistence and browser performance remain unverified. Frozen native binary
      adcaa1a4 uses source 01638c43 with the runtime tree merged as df645359.
    status: unknown
    evidence: https://github.com/benletchford/systemless/issues/6059#issuecomment-6788785138
runtime:
  executable_path: CM 9900 Demo/CM 9900 demo
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: sha256
    sha256: cbc5c656d9c302d77bb08a3ca4174aad8cdc2561d31fcd23803b53a02f927f94
    size_bytes: 63905070
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/championship-manager-season-9900
    - https://archive.org/details/macworld-uk-cd-january-2000
    - https://github.com/benletchford/systemless/issues/6059
    license: Maintainer-accepted promotional demo
    rights_holder: Sports Interactive, Eidos Interactive and original contributors
    permission: >-
      Accepted under the maintainer-approved project policy for complete, unchanged,
      period-distributed promotional demos. This records project-policy acceptance
      without claiming an affirmative publisher licence grant. No applicable explicit
      redistribution restriction was identified in reviewed received-package notices.
      Preserve the complete archive, notices and demo limits.
    notes: "The received SIT is preserved unchanged. All 202 core data forks and 545 resource payloads match the original January 2000 Macworld UK disc installer, with no missing or extra core files. The later archive wrapper is not period-byte-identical: 138 resource-map bookkeeping states and 24 Finder flags differ, while resource names, IDs, attributes and Finder type/creator match. Independent unar extraction matches production packing on all 202 files, 404 forks and 202 Finder records. Reviewed selected received UI, text and version notices and complete SI Symbol font naming records. The original outer installer's custom-compressed templates are not present in the received SIT and were not decoded or claimed reviewed."
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: c0447efc57c40d81048dcbda925adbd99a5af4bcda3c5097298a084986a25089
    size_bytes: 251597
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6059
    rights_holder: Sports Interactive, Eidos Interactive and original contributors
    permission: >-
      Fresh content-only Systemless capture of the project-policy-accepted demo.
      Underlying artwork retains its original copyright.
    notes: >-
      Unscaled 800x600 original full-screen game surface, with no Mac menu bar,
      desktop or emulator framing. Shows the board-expectations message after taking control
      of Arsenal and using Continue Game.
references:
- https://github.com/benletchford/systemless/issues/6059
---

![Arsenal board expectations in Championship Manager 99/00](https://assets.systemless.org/catalogue/media/sha256/c0/c0447efc57c40d81048dcbda925adbd99a5af4bcda3c5097298a084986a25089.png)

Choose a club and take responsibility for its squad in the original 3.5 demo.
The complete original demo and its limits are preserved. Browser qualification
is pending.
