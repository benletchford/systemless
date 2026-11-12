---
id: deer-avenger-demo
kind: game
title: Deer Avenger Demo
summary: Turn the tables on hunters in the original Macintosh Deer Avenger demo.
developer: Hypnotix / Westlake Interactive
year: 1999
architectures:
- ppc
default_architecture: ppc
category: FPS
compatibility:
  status: unknown
  verified:
  - date: "2026-11-13"
    tester: Catalogue maintainer
    systemless_version: 3e240c25 native
    architecture: ppc
    environment: >-
      Complete unchanged period installer, with original preferred partition,
      display, sound, CPU and preferences. New Hunt, default M16, Next, default
      Minnesota, Hunt, then map Hunt. Click Aim and then the hunting view to fire.
      Active and omission runs complete 1448 frontend and guest ticks with zero
      exhausted frames; armed and pre-input RGB captures match exactly. Only the
      active run shows Bambo's muzzle flash, gun recoil and ejected casing.
      Actual captures inspected. Bounded startup and firing response only; hunter
      hits, trophies, full-game behavior, audio fidelity, independent hardware
      accuracy and browser pacing remain unverified. Frozen native binary SHA256
      86f08e1a329b3e1e5b8fb88582c838409a12760d2e18783dd1801f1d2207b6a2.
    status: playable
    evidence: https://github.com/benletchford/systemless/issues/6206
runtime:
  executable_path: Deer Avenger Demo/Deer Avenger Demo
artifacts:
- id: archive
  role: archive
  format: bin
  source:
    type: url
    url: https://discmaster.textfiles.com/file/17741/MacAddict_035_1999_07.iso/Software/Fun%20%26%20Games/Deer%20Avenger%20Demo
    download_page: https://classicmacdemos.com/deer-avenger
    expected_sha256: ef0f31d0bd7f631339e35df137bdf37da257950106dac8035a4425241076887f
    expected_size: 9857280
  provenance:
    redistribution: permitted
    original: true
    sources:
    - https://classicmacdemos.com/deer-avenger
    - https://archive.org/details/macaddict-cd-035
    - https://github.com/benletchford/systemless/issues/6206
    license: Maintainer-accepted promotional demo
    rights_holder: Simon & Schuster, Hypnotix, Westlake Interactive and original contributors
    permission: >-
      Accepted under the user-authorized existing project policy for complete
      unchanged original promotional demos from period cover-disc distribution.
      This records project-policy acceptance, not an affirmative publisher or
      artwork licence grant. No applicable explicit redistribution restriction was
      identified in the reviewed notices. Preserve every original file, notice
      and demo limit.
    notes: >-
      Complete original MacAddict 035 July 1999 installer in a public MacBinary
      export. Both original installer forks match the full period ISO, whose
      published size and SHA1 were verified. The MacBinary wrapper is not claimed
      to be publisher-authored. All 103 installed files, including the original
      DrawSprocket library, are retained. All 206 raw forks match independent
      extraction; every declared fork CRC, path, type, creator and Finder flag
      passes. The full 10352-byte ReadMe and 47 standard installer/component text,
      UI, string and version resources were reviewed without parse errors. PICT
      conversions were incomplete; exhaustive graphical notice review is not
      claimed. No opaque game logic, Smacker video, WAV or DAT content was
      interpreted. No files were stripped, replaced, reconstructed or modified.
      Diagnostic Compact Pro bridges and separately installed files are not
      hosting candidates. Requires the generic raw-RLE tail decoder fix in PR6205.
- id: gameplay-screenshot
  role: screenshot
  format: png
  source:
    type: incoming
    path: catalogue/incoming/deer-avenger-demo/gameplay.png
  provenance:
    redistribution: permitted
    content_only: true
    sources:
    - https://github.com/benletchford/systemless/issues/6206
    rights_holder: Simon & Schuster, Hypnotix, Westlake Interactive and original contributors
    permission: Real content-only capture of the project-policy-accepted promotional demo.
    notes: >-
      Unscaled and unretouched 640x480 lossless capture of actual native gameplay
      during a firing input. Contains only the original game view and HUD, with no
      desktop or menu bar. No separate artwork licence grant claimed.
references:
- https://classicmacdemos.com/deer-avenger
- https://archive.org/details/macaddict-cd-035
- https://github.com/benletchford/systemless/issues/6206
---

Play as Bambo in the original Macintosh **Deer Avenger** demo.

Choose **New Hunt**, select a weapon and hunting area, then choose a spot on the
map and click **Hunt**. Move the pointer to the edges to look around. Click
**Aim**, then click the view to fire; **Space** or **Escape** lowers the weapon.
The bundled ReadMe explains calls, binoculars and trophies.

Native startup and firing response are verified. Browser qualification remains pending.

![Bambo firing during a snowy hunt](incoming/deer-avenger-demo/gameplay.png)
