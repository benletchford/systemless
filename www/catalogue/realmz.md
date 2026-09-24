---
id: realmz
kind: game
title: Realmz
summary: >-
  Begin a fantasy adventure in Fantasoft's original Macintosh 5.1 shareware release.
developer: Fantasoft / Tim Phillips
publisher: Fantasoft
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: false
compatibility:
  status: boots
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: "local runtime with corrected StuffIt codebook-four decoder"
    architecture: 68k
    environment: >-
      The unchanged Info-Mac BinHex distribution loaded its original 68K application,
      opened the shareware scenario and character files, showed Maximum Levels = 18,
      added the sample character Kevlar, and reached City of Bywater gameplay in a
      deterministic run. The corrected scenario resource fork matches an independent
      extraction byte-for-byte. Browser launch remains unapproved.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2695
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: url
    url: https://ftp.funet.fi/pub/mac/info-mac/game/adv/rlmz/realmz-51.hqx
    expected_sha256: 00217bfa83bbebd1eb4deac00ab904877c8ee601adc638e3cef2d48737ee2720
    expected_size: 10746107
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/adv/rlmz/realmz-51.hqx
    - https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/adv/rlmz/
    license: Fantasoft Software License bundled with Realmz 5.1
    rights_holder: Fantasoft / Tim Phillips
    permission: >-
      The bundled Fantasoft Software License permits non-profit distribution of
      the complete, unmodified software without prior written notice. This is the
      unchanged original 5.1 BinHex distribution, including the application,
      scenarios, characters, licence, and documentation; it contains no added
      registration code or modified game files.
    notes: >-
      The original 1998 Info-Mac file is 10,746,107 bytes, SHA-256
      00217bfa83bbebd1eb4deac00ab904877c8ee601adc638e3cef2d48737ee2720.
      The archive is BinHex-wrapped StuffIt and needs the codebook-four decoder
      published in stuffit 0.3.0 for correct scenario data.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/realmz/gameplay.png
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2695
    permission: >-
      Fresh deterministic Systemless capture of the unregistered shareware
      City of Bywater scene. Underlying artwork remains Fantasoft's property.
    notes: >-
      Cropped the 641-by-460 game content surface at (79,80) from the
      800-by-600 guest framebuffer, excluding the menu bar, desktop, and window
      frame without changing game pixels. PNG SHA-256
      5dc09e6442c75504f806b871f8cdb041d5e87f054642d7a1213a3e876a7e0b4a;
      176,117 bytes.
references:
- https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/adv/rlmz/
- https://ftp.funet.fi/pub/mac/info-mac/game/adv/rlmz/realmz-51.hqx
- https://github.com/benletchford/systemless/issues/2695
---

## Begin in Bywater

![City of Bywater in the original Realmz shareware release](incoming/realmz/gameplay.png)

Realmz is Fantasoft's Macintosh fantasy role-playing game. Begin with the
included City of Bywater scenario, build a party, and explore its town and
surrounding encounters. This entry uses the original, complete 5.1 shareware
distribution; registration and any restrictions on additional scenarios are
unchanged. The bundled licence permits non-profit distribution of complete,
unmodified copies. The game and artwork remain Fantasoft's property.
