---
id: myth-ii-demo
kind: game
title: 'Myth II: Soulblighter Demo'
summary: Command small armies in Bungie's three-dimensional tactical battlefield demo.
developer: Bungie Software Products Corporation
publisher: Bungie Software Products Corporation
year: 1998
architectures:
- ppc
default_architecture: ppc
category: Strategy
launch_enabled: false
compatibility:
  status: unknown
runtime:
  executable_path: Myth II Demo/Myth II Demo
  show_menu_bar: true
artifacts:
- id: archive
  role: archive
  format: sit
  source:
    type: incoming
    path: catalogue/incoming/myth-ii-demo/original.sit
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/download/myth-ii-soulblighter/
    - https://www.vintageapplemac.com/files/misc/9904.pdf
    - https://github.com/benletchford/systemless/issues/4132
    - https://github.com/benletchford/systemless/issues/5920
    rights_holder: Bungie Software Products Corporation (original 1999 notice)
    license: Maintainer-accepted freely distributed promotional demo
    permission: >-
      Accepted under the project's promotional-demo policy with explicit
      maintainer authorization on 2026-11-10. Period cover-disc distribution is
      documented, the complete original downloaded package is preserved, and no
      explicit redistribution restriction was found in inspected applicable
      notices. This is project-policy acceptance, not an affirmative publisher
      licence grant or a claim about current ownership. Preserve all notices and
      bundled components. No modern port or Marathon licence is substituted.
    notes: >-
      Original 38894556-byte SIT, SHA-256
      28d2141e29ce013dcc0a629d998884d61d0a709105eaa44f4352bdd5227cc78c,
      matching the earlier tested download in #4132. All 8 files, 16 forks and
      8 Finder records match independent unar extraction. Strict web-pack end
      68992947, SHA-256
      72d0ee7d828b5c8fc46a5e98d89382270a313b485d45a5f1f3e54d401e3f39e7.
      Includes Read Me, SMACKW32.DLL and all five tag files unchanged. The complete
      30645-byte Read Me identifies demo v1.2.1 and copyright 1999; 63 selected
      UI/text/version resources and retained notice matches were inspected.
      Macworld April 1999, printed page 55, documents the playable demo on its
      March cover disc. That earlier disc installer has a different executable
      size; no exact publisher-media byte parity is claimed for this download.
- id: title-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/myth-ii-demo/title.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/5920
    rights_holder: Bungie Software Products Corporation (original artwork)
    permission: >-
      Fresh Systemless capture from the project-policy-accepted promotional demo;
      original artwork retains its copyright.
    notes: "Original 640x480 PLAYABLE DEMO title surface, cropped from the top-left of the native capture to remove surrounding guest desktop; no resampling, colour editing or host UI. SHA-256 5c167deece844bb0b73dcbf7068aa8d939c4d5dc570605a89aa7a5c526817839. This is a splash screen, not gameplay evidence."
references:
- https://github.com/benletchford/systemless/issues/5920
- https://github.com/benletchford/systemless/issues/4132
---

![Myth II playable demo title screen](incoming/myth-ii-demo/title.png)

Command small groups of warriors, archers and dwarves across a three-dimensional
battlefield. The original demo describes a tutorial, two single-player levels
and one network map.

A bounded native documentation capture reaches the original PLAYABLE DEMO splash
after ordinary Return at 362 frontend/guest ticks, with zero assertions and zero
budget exhaustion. This documents the title screen only. The original PowerPC
executable, default CPU, partition, sound and initial display are preserved.
Tutorial loading (#4132), actual gameplay, browser pacing and audio fidelity
remain unqualified, so launch stays disabled.
