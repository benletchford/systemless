---
id: realmz
kind: game
title: Realmz
summary: >-
  Begin a fantasy adventure in Fantasoft's original Macintosh 5.1 shareware
  release.
developer: Fantasoft / Tim Phillips
publisher: Fantasoft
year: 1994
architectures:
- 68k
default_architecture: 68k
category: Role-Playing
launch_enabled: true
compatibility:
  status: playable
  verified:
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: 0.59.0 + stuffit 0.3.0 release-mode browser build
    architecture: 68k
    environment: >-
      Chromium loaded the exact promoted BinHex archive without console errors.
      The registration screen rendered, and choosing Not Yet advanced to the
      Realmz title card. Two 12-second samples achieved only 16-18 guest
      ticks/second at about 55-56 host frames/second, with long runtime frames
      and a low audio queue. This historical failure preceded the generic pacing
      fixes and later browser qualification recorded below.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2699
  - date: "2026-09-24"
    tester: Catalogue maintainer
    systemless_version: local runtime with corrected StuffIt codebook-four decoder
    architecture: 68k
    environment: >-
      The unchanged Info-Mac BinHex distribution loaded its original 68K application,
      opened the shareware scenario and character files, showed Maximum Levels = 18,
      added the sample character Kevlar, and reached City of Bywater gameplay in a
      deterministic run. The corrected scenario resource fork matches an independent
      extraction byte-for-byte. This historical native check preceded browser approval.
    status: boots
    evidence: https://github.com/benletchford/systemless/issues/2695
  - date: "2026-10-31"
    tester: Catalogue maintainer
    systemless_version: 856f5f79d721ce383c4cd2e93db2165da90d117d
    architecture: 68k
    environment: >-
      The original hosted archive was fetched without interception and matched its
      10746107-byte SHA-256 receipt. In a production-origin Chrome 156 preview,
      frontend files were substituted and the worker bootstrap loaded the identical
      local Wasm through a data URL; the runtime used a 68K worker, WebGL and 25 MHz.
      Registration was dismissed, Kevlar joined the party, and ordinary Down input
      moved in City of Bywater from X2/Y1 at 12:00 to X2/Y3 at 12:20. Inspected
      captures confirm the map and movement. A 20-second active-map sample reached
      60.202 guest ticks and 60.652 worker replies per second, with no recorded browser
      errors. Launch is approved for this bounded gameplay. Full scenario, combat,
      saves, browser audio fidelity and an independent deployed replay remain unverified.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/2699
artifacts:
- id: archive
  role: archive
  format: hqx
  source:
    type: sha256
    sha256: 00217bfa83bbebd1eb4deac00ab904877c8ee601adc638e3cef2d48737ee2720
    size_bytes: 10746107
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://ftp.funet.fi/pub/mac/info-mac/game/adv/rlmz/realmz-51.hqx
    - https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/adv/rlmz/
    license: Fantasoft Software License bundled with Realmz 5.1
    rights_holder: Fantasoft / Tim Phillips
    permission: >-
      The bundled Fantasoft Software License permits non-profit distribution of the
      complete, unmodified software without prior written notice. This is the unchanged
      original 5.1 BinHex distribution, including the application, scenarios,
      characters, licence, and documentation; it contains no added registration code or modified
      game files.
    notes: >-
      The original 1998 Info-Mac file is 10,746,107 bytes, SHA-256
      00217bfa83bbebd1eb4deac00ab904877c8ee601adc638e3cef2d48737ee2720. The archive is BinHex-wrapped
      StuffIt and needs the codebook-four decoder published in stuffit 0.3.0 for correct
      scenario data.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: sha256
    sha256: 5dc09e6442c75504f806b871f8cdb041d5e87f054642d7a1213a3e876a7e0b4a
    size_bytes: 176117
  provenance:
    redistribution: permitted
    original: true
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/2695
    permission: >-
      Fresh deterministic Systemless capture of the unregistered shareware City of
      Bywater scene. Underlying artwork remains Fantasoft's property.
    notes: >-
      Cropped the 641-by-460 game content surface at (79,80) from the 800-by-600
      guest framebuffer, excluding the menu bar, desktop, and window frame without changing
      game pixels. PNG SHA-256
      5dc09e6442c75504f806b871f8cdb041d5e87f054642d7a1213a3e876a7e0b4a; 176,117 bytes.
references:
- https://www.nic.funet.fi/pub/files/index/mac/info-mac/game/adv/rlmz/
- https://ftp.funet.fi/pub/mac/info-mac/game/adv/rlmz/realmz-51.hqx
- https://github.com/benletchford/systemless/issues/2695
- https://github.com/benletchford/systemless/issues/2699
---

## Begin in Bywater

![City of Bywater in the original Realmz shareware release](https://assets.systemless.org/catalogue/media/sha256/5d/5dc09e6442c75504f806b871f8cdb041d5e87f054642d7a1213a3e876a7e0b4a.png)

Realmz is Fantasoft's Macintosh fantasy role-playing game. Begin with the
included City of Bywater scenario, build a party, and explore its town and
surrounding encounters. This entry uses the original, complete 5.1 shareware
distribution; registration and any restrictions on additional scenarios are
unchanged. The bundled licence permits non-profit distribution of complete,
unmodified copies. The game and artwork remain Fantasoft's property.
The opening town and movement are verified; combat, saves, and later scenario
progress have not yet been verified.
